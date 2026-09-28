//! Background task: bus transitions (and a periodic disk check) →
//! notifications, throttled, stored, pushed to the UI and the webhook.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sqlx::PgPool;
use tokio::sync::broadcast::error::RecvError;

use super::rules::{self, Draft};
use super::{repo, webhook};
use crate::bus::{Bus, BusEvent};

const DISK_CHECK: Duration = Duration::from_secs(300);
/// Cameras going offline (or coming back) within this long of the first
/// one are told together: a switch or a power cut takes many down at once.
#[cfg(not(test))]
const BURST_WINDOW: Duration = Duration::from_secs(10);
#[cfg(test)]
const BURST_WINDOW: Duration = Duration::from_millis(200);

/// Offline (or back online) notifications waiting for the burst window.
struct Batch {
    since: Instant,
    /// Camera name and its own notification.
    items: Vec<(String, Draft)>,
    webhook: Option<String>,
    nvr_name: String,
}

struct Notifier {
    db: PgPool,
    bus: Bus,
    bind: SocketAddr,
    recordings: Arc<crate::recordings::RecordingFiles>,
    last_sent: HashMap<(&'static str, Option<String>), Instant>,
    offline_notified: HashSet<String>,
    batches: HashMap<&'static str, Batch>,
}

/// Subscribe now and run in the background.
pub fn start(db: PgPool, bus: Bus, bind: SocketAddr, recordings: Arc<crate::recordings::RecordingFiles>) {
    let events = bus.subscribe();
    let n = Notifier { db, bus, bind, recordings, last_sent: HashMap::new(), offline_notified: HashSet::new(), batches: HashMap::new() };
    tokio::spawn(n.run(events));
}

impl Notifier {
    async fn run(mut self, mut events: tokio::sync::broadcast::Receiver<BusEvent>) {
        let mut disk = tokio::time::interval(DISK_CHECK);
        let mut bursts = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = bursts.tick() => self.flush_bursts().await,
                event = events.recv() => match event {
                    Ok(e) => self.on_event(&e).await,
                    Err(RecvError::Lagged(n)) => tracing::warn!("notifier missed {n} bus messages"),
                    Err(RecvError::Closed) => return,
                },
                _ = disk.tick() => self.check_disk().await,
            }
        }
    }

    async fn on_event(&mut self, e: &BusEvent) {
        let camera = match e {
            BusEvent::CameraOffline { camera_id, .. }
            | BusEvent::CameraOnline { camera_id, .. }
            | BusEvent::DetectionStarted { camera_id, .. }
            | BusEvent::RecordingStopped { camera_id, .. } => camera_id.clone(),
            _ => return,
        };
        let Ok(settings) = crate::settings::load_app(&self.db, self.bind).await else { return };
        let cam = crate::cameras::repo_get(&self.db, &camera).await.ok().flatten();
        let name = cam.as_ref().map_or_else(|| camera.clone(), |c| c.name.clone());
        let facts = rules::CameraFacts { name: &name, was_offline: self.offline_notified.contains(&camera), notify_motion: cam.as_ref().is_some_and(|c| c.motion.notify) };
        let Some(draft) = rules::draft(e, &settings.notifications, &facts) else { return };
        match draft.kind {
            "camera_offline" => {
                self.offline_notified.insert(camera.clone());
            }
            "camera_online" => {
                self.offline_notified.remove(&camera);
            }
            _ => {}
        }
        if matches!(draft.kind, "camera_offline" | "camera_online") {
            // Held a few seconds: others may follow (see `flush_bursts`).
            if !self.cooling_down(&draft) {
                let batch = self.batches.entry(draft.kind).or_insert_with(|| Batch { since: Instant::now(), items: Vec::new(), webhook: None, nvr_name: String::new() });
                batch.webhook = settings.notifications.webhook_url.clone();
                batch.nvr_name = settings.general.nvr_name.clone();
                batch.items.push((name, draft));
            }
            return;
        }
        self.raise(draft, settings.notifications.webhook_url.as_deref(), &settings.general.nvr_name).await;
    }

    /// Batches whose window is over: one camera goes out as before, several
    /// as one notification ("12 cameras are back online").
    async fn flush_bursts(&mut self) {
        let due: Vec<&'static str> = self.batches.iter().filter(|(_, b)| b.since.elapsed() >= BURST_WINDOW).map(|(k, _)| *k).collect();
        for kind in due {
            let Some(mut batch) = self.batches.remove(kind) else { continue };
            if batch.items.len() == 1 {
                let (_, draft) = batch.items.remove(0);
                self.raise(draft, batch.webhook.as_deref(), &batch.nvr_name).await;
                continue;
            }
            // Each camera's own cooldown counts as used.
            for (_, d) in &batch.items {
                self.last_sent.insert((d.kind, d.camera_id.clone()), Instant::now());
            }
            self.send(rules::burst(kind, &batch.items), batch.webhook.as_deref(), &batch.nvr_name).await;
        }
    }

    fn cooling_down(&self, draft: &Draft) -> bool {
        self.last_sent.get(&(draft.kind, draft.camera_id.clone())).is_some_and(|t| t.elapsed() < draft.cooldown())
    }

    async fn check_disk(&mut self) {
        let Ok(settings) = crate::settings::load_app(&self.db, self.bind).await else { return };
        let Some((free, threshold)) = crate::storage::low_space(&self.db, &self.recordings.root()).await else { return };
        if let Some(draft) = rules::storage_low(&settings.notifications, free, threshold) {
            self.raise(draft, settings.notifications.webhook_url.as_deref(), &settings.general.nvr_name).await;
        }
    }

    async fn raise(&mut self, draft: Draft, webhook_url: Option<&str>, nvr_name: &str) {
        if self.cooling_down(&draft) {
            return;
        }
        self.send(draft, webhook_url, nvr_name).await;
    }

    /// Store, push to the UI and the webhook (no cooldown check).
    async fn send(&mut self, draft: Draft, webhook_url: Option<&str>, nvr_name: &str) {
        let key = (draft.kind, draft.camera_id.clone());
        let stored = match repo::insert(&self.db, &draft).await {
            Ok(n) => n,
            // Not stored: the next one of this kind may try again.
            Err(e) => {
                tracing::warn!("cannot store notification: {e}");
                return;
            }
        };
        self.last_sent.insert(key, Instant::now());
        self.bus.publish(BusEvent::NotificationsChanged);
        if let Some(url) = webhook_url {
            let (url, name) = (url.to_string(), nvr_name.to_string());
            tokio::spawn(async move {
                let _ = webhook::send(&url, &stored, &name).await;
            });
        }
    }
}
