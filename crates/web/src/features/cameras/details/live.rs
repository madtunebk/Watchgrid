use leptos::html::Div;
use leptos::prelude::*;

use crate::api::{Camera, CameraStatus};
use crate::features::cameras::widgets::{CameraPreview, RecordButton};
use crate::format;
use crate::ui::{I, Icon, fullscreen, snapshot as snap};

#[component]
pub fn LiveTab(#[prop(into)] camera: Signal<Camera>) -> impl IntoView {
    let stage = NodeRef::<Div>::new();
    let muted = RwSignal::new(true);
    // Only streams with sound get the mute button.
    let has_audio = RwSignal::new(false);
    let snapshot = RwSignal::new(None::<String>);

    let fullscreen = move |_| {
        if let Some(el) = stage.get() {
            fullscreen::toggle(&el);
        }
    };
    let take_snapshot = move |_| {
        let Some(el) = stage.get_untracked() else { return };
        let name = snap::file_name(&camera.get_untracked().name);
        let at = crate::format::time_hms(chrono::Local::now());
        snapshot.set(Some(match snap::save_frame(&el, &name) {
            Ok(()) => format!("Snapshot saved at {at} ({name}.jpg)"),
            Err(e) => e.to_string(),
        }));
    };

    view! {
        <div class="live-tab">
            <div class="live-stage" node_ref=stage>
                {move || view! { <CameraPreview camera=camera.get() muted=muted has_audio /> }}
            </div>

            <div class="live-controls">
                {move || {
                    let c = camera.get();
                    let available = c.enabled && c.status == CameraStatus::Online;
                    view! { <RecordButton camera_id=c.id.clone() recording=c.recording_active available /> }
                }}
                <button class="btn btn--secondary btn--sm" on:click=fullscreen>
                    <Icon icon=I::Maximize class="icon icon--sm" />"Fullscreen"
                </button>
                <Show when=move || has_audio.get()>
                    <button class="btn btn--secondary btn--sm" aria-pressed=move || (!muted.get()).to_string()
                        on:click=move |_| muted.update(|m| *m = !*m)>
                        {move || if muted.get() {
                            view! { <Icon icon=I::VolumeOff class="icon icon--sm" />"Unmute" }.into_any()
                        } else {
                            view! { <Icon icon=I::Volume class="icon icon--sm" />"Mute" }.into_any()
                        }}
                    </button>
                </Show>
                <button class="btn btn--secondary btn--sm" on:click=take_snapshot>
                    <Icon icon=I::Camera class="icon icon--sm" />"Snapshot"
                </button>
                {move || snapshot.get().map(|s| view! { <span class="live-controls__note">{s}</span> })}
            </div>

            {move || {
                let c = camera.get();
                let s = &c.main_stream;
                let online = c.enabled && c.status == CameraStatus::Online;
                let uptime = c.connected_since.map(|t| format::uptime((chrono::Utc::now() - t).num_seconds().max(0) as u64));
                let dash = || "—".to_string();
                let items = vec![
                    ("Status", crate::features::cameras::labels::connection(&c).1.to_string()),
                    ("Resolution", s.height.map(|h| format!("{h}p")).unwrap_or_else(dash)),
                    ("Frame rate", s.fps.map(|f| format!("{f:.0} FPS")).unwrap_or_else(dash)),
                    ("Codec", s.codec.clone().unwrap_or_else(dash)),
                    ("Bitrate", s.bitrate.map(|b| format!("{:.1} Mbit/s", b as f32 / 1000.0)).unwrap_or_else(dash)),
                    ("Audio", s.audio_codec.clone().unwrap_or_else(|| "None".into())),
                    ("Connected for", if online { uptime.unwrap_or_else(dash) } else { dash() }),
                ];
                view! {
                    <dl class="info-strip">
                        {items.into_iter().enumerate().map(|(i, (k, v))| view! {
                            <div class:info-strip__status=i == 0 class:info-strip__status--online=i == 0 && online>
                                <dt>{k}</dt><dd>{v}</dd>
                            </div>
                        }).collect_view()}
                    </dl>
                }
            }}
        </div>
    }
}
