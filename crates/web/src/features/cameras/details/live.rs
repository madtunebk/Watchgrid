use leptos::html::Div;
use leptos::prelude::*;

use crate::api::{Camera, CameraStatus};
use crate::features::cameras::widgets::{CameraPreview, PtzPad, RecordButton};
use crate::format;
use crate::ui::{I, Icon, Tone, fullscreen, snapshot as snap, use_toaster};

#[component]
pub fn LiveTab(#[prop(into)] camera: Signal<Camera>) -> impl IntoView {
    let stage = NodeRef::<Div>::new();
    let muted = RwSignal::new(true);
    // Only streams with sound get the mute button.
    let has_audio = RwSignal::new(false);
    let toaster = use_toaster();

    let fullscreen = move |_| {
        if let Some(el) = stage.get() {
            fullscreen::toggle(&el);
        }
    };
    let take_snapshot = move |_| {
        let Some(el) = stage.get_untracked() else { return };
        let name = snap::file_name(&camera.get_untracked().name);
        match snap::save_frame(&el, &name) {
            Ok(()) => toaster.show(Tone::Online, format!("Snapshot saved ({name}.jpg)")),
            Err(e) => toaster.show(Tone::Danger, e.to_string()),
        }
    };

    // The same controls as a camera opened in Live View: a bar over the
    // bottom of the picture and the pan / tilt arrows (shown on hover, and
    // always on touch screens); saved positions stay under the video.
    view! {
        <div class="live-tab">
            <div class="live-stage tile" node_ref=stage>
                {move || view! { <CameraPreview camera=camera.get() muted=muted has_audio /> }}
                <div class="tile__ptz"><PtzPad camera_id=camera.get_untracked().id compact=true /></div>
                <div class="tile__controls">
                    {move || {
                        let c = camera.get();
                        let available = c.enabled && c.status == CameraStatus::Online;
                        view! { <RecordButton camera_id=c.id.clone() recording=c.recording_active reason=c.recording_reason available compact=true /> }
                    }}
                    <button class="tile__btn" title="Snapshot" aria-label="Snapshot" on:click=take_snapshot>
                        <Icon icon=I::Camera class="icon icon--sm" />
                    </button>
                    <Show when=move || has_audio.get()>
                        <button class="tile__btn" aria-label=move || if muted.get() { "Unmute" } else { "Mute" }
                            title=move || if muted.get() { "Unmute" } else { "Mute" }
                            on:click=move |_| muted.update(|m| *m = !*m)>
                            {move || view! { <Icon icon=if muted.get() { I::VolumeOff } else { I::Volume } class="icon icon--sm" /> }}
                        </button>
                    </Show>
                    <span class="tile__spacer"></span>
                    <button class="tile__btn" title="Fullscreen" aria-label="Fullscreen" on:click=fullscreen>
                        <Icon icon=I::Maximize class="icon icon--sm" />
                    </button>
                </div>
            </div>
            <PtzPad camera_id=camera.get_untracked().id presets_only=true />

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
