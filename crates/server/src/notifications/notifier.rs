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

struct Notifier {
    db: PgPool,
    bus: Bus,
    bind: SocketAddr,
    recordings: Arc<crate::recordings::RecordingFiles>,
    last_sent: HashMap<(&'static str, Option<String>), Instant>,
    offline_notified: HashSet<String>,
}

/// Subscribe now and run in the background.
pub fn start(db: PgPool, bus: Bus, bind: SocketAddr, recordings: Arc<crate::recordings::RecordingFiles>) {
    let events = bus.subscribe();
    let n = Notifier { db, bus, bind, recordings, last_sent: HashMap::new(), offline_notified: HashSet::new() };
    tokio::spawn(n.run(events));
}

impl Notifier {
    async fn run(mut self, mut events: tokio::sync::broadcast::Receiver<BusEvent>) {
        let mut disk = tokio::time::interval(DISK_CHECK);
        loop {
            tokio::select! {
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
        self.raise(draft, settings.notifications.webhook_url.as_deref(), &settings.general.nvr_name).await;
    }

    async fn check_disk(&mut self) {
        let Ok(settings) = crate::settings::load_app(&self.db, self.bind).await else { return };
        let Some((free, threshold)) = crate::storage::low_space(&self.db, &self.recordings.root()).await else { return };
        if let Some(draft) = rules::storage_low(&settings.notifications, free, threshold) {
            self.raise(draft, settings.notifications.webhook_url.as_deref(), &settings.general.nvr_name).await;
        }
    }

    async fn raise(&mut self, draft: Draft, webhook_url: Option<&str>, nvr_name: &str) {
        let key = (draft.kind, draft.camera_id.clone());
        if self.last_sent.get(&key).is_some_and(|t| t.elapsed() < draft.cooldown()) {
            return;
        }
        self.last_sent.insert(key, Instant::now());
        let stored = match repo::insert(&self.db, &draft).await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!("cannot store notification: {e}");
                return;
            }
        };
        self.bus.publish(BusEvent::NotificationsChanged);
        if let Some(url) = webhook_url {
            let (url, name) = (url.to_string(), nvr_name.to_string());
            tokio::spawn(async move {
                let _ = webhook::send(&url, &stored, &name).await;
            });
        }
    }
}
