//! State and display of the "Test …" buttons.

use std::future::Future;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{ApiResult, ConnectionProbe, OnvifProbe, StreamProbe};
use crate::features::events::EventChip;
use crate::ui::{I, Icon};

#[derive(Clone, PartialEq)]
pub enum Probe<T> {
    Idle,
    Running,
    Done(T),
    Failed(String),
}

/// Run `test` and store its progress in `state`.
pub fn run<T, F>(state: RwSignal<Probe<T>>, test: F)
where
    T: Send + Sync + 'static,
    F: Future<Output = ApiResult<T>> + 'static,
{
    state.set(Probe::Running);
    spawn_local(async move {
        state.set(match test.await {
            Ok(v) => Probe::Done(v),
            Err(e) => Probe::Failed(e.to_string()),
        });
    });
}

/// Shared frame: spinner / success / failure header plus details.
fn frame(ok: bool, message: String, details: AnyView) -> AnyView {
    view! {
        <div class="probe" class:probe--ok=ok class:probe--fail=!ok role="status">
            <div class="probe__head">
                <Icon icon=if ok { I::Check } else { I::TriangleAlert } class="icon icon--sm" />
                <span>{message}</span>
            </div>
            {details}
        </div>
    }
    .into_any()
}

fn running(what: &'static str) -> AnyView {
    view! {
        <div class="probe probe--running" role="status">
            <div class="probe__head"><Icon icon=I::Loader class="icon icon--sm spin" /><span>{what}</span></div>
        </div>
    }
    .into_any()
}

fn facts(rows: Vec<(&'static str, String)>) -> AnyView {
    view! {
        <dl class="probe__facts">
            {rows.into_iter().map(|(k, v)| view! { <div><dt>{k}</dt><dd>{v}</dd></div> }).collect_view()}
        </dl>
    }
    .into_any()
}

pub fn connection_view(p: Probe<ConnectionProbe>) -> AnyView {
    match p {
        Probe::Idle => ().into_any(),
        Probe::Running => running("Contacting camera…"),
        Probe::Failed(e) => frame(false, e, ().into_any()),
        Probe::Done(r) => {
            let mut rows = vec![];
            if let Some(d) = r.device {
                rows.push(("Device", d));
            }
            if let Some(l) = r.latency_ms {
                rows.push(("Latency", format!("{l} ms")));
            }
            frame(r.ok, r.message, facts(rows))
        }
    }
}

pub fn stream_view(p: Probe<StreamProbe>) -> AnyView {
    match p {
        Probe::Idle => ().into_any(),
        Probe::Running => running("Testing stream…"),
        Probe::Failed(e) => frame(false, e, ().into_any()),
        Probe::Done(r) if !r.ok => frame(false, r.message, ().into_any()),
        Probe::Done(r) => {
            let res = match (r.width, r.height) {
                (Some(w), Some(h)) => format!("{w}×{h}"),
                _ => "—".into(),
            };
            frame(
                true,
                r.message,
                facts(vec![
                    ("Codec", r.codec.unwrap_or_else(|| "—".into())),
                    ("Resolution", res),
                    ("FPS", r.fps.map(|f| format!("{f:.0}")).unwrap_or_else(|| "—".into())),
                    ("Audio", r.audio_codec.unwrap_or_else(|| "None".into())),
                    ("Latency", r.latency_ms.map(|l| format!("{l} ms")).unwrap_or_else(|| "—".into())),
                ]),
            )
        }
    }
}

pub fn onvif_view(p: Probe<OnvifProbe>) -> AnyView {
    match p {
        Probe::Idle => ().into_any(),
        Probe::Running => running("Querying ONVIF and trying event delivery (up to ~10 s)…"),
        Probe::Failed(e) => frame(false, e, ().into_any()),
        Probe::Done(r) if !r.ok => frame(false, r.message, ().into_any()),
        Probe::Done(r) => frame(
            true,
            r.message,
            view! {
                {r.delivery.map(|d| view! {
                    <p class=if d.ok { "probe__delivery text-online" } else { "probe__delivery text-danger" }>
                        <span class="probe__label">"Event delivery"</span>
                        {d.message}
                    </p>
                })}
                <div class="probe__detections">
                    <span class="probe__label">"Detections"</span>
                    {r.detections.into_iter().map(|kind| view! { <EventChip kind /> }).collect_view()}
                </div>
                <ul class="probe__topics mono">
                    {r.event_topics.into_iter().map(|t| view! { <li>{t}</li> }).collect_view()}
                </ul>
            }
            .into_any(),
        ),
    }
}
