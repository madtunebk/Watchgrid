use leptos::prelude::*;
use leptos_router::components::A;

use super::save;
use crate::api::{Camera, Stream, StreamRole, StreamStatus};
use crate::ui::{SaveBar, SaveState};
use crate::ui::form::{Choice, FormSection, RadioCards};
use crate::ui::{Badge, Tone};

#[component]
pub fn StreamTab(#[prop(into)] camera: Signal<Camera>) -> impl IntoView {
    let cam = camera.get_untracked();
    let has_sub = cam.sub_stream.is_some();
    let record = RwSignal::new(cam.recording.stream);
    let motion = RwSignal::new(cam.motion.stream);
    let state = SaveState::new();

    let saved = move || {
        let c = camera.get();
        (c.recording.stream, c.motion.stream)
    };
    let dirty = Signal::derive(move || (record.get(), motion.get()) != saved());
    let on_save = Callback::new(move |_| {
        let (r, m) = (record.get_untracked(), motion.get_untracked());
        save::camera(state, &camera.get_untracked(), |i| {
            i.recording.stream = r;
            i.motion.stream = m;
        });
    });
    let on_revert = Callback::new(move |_| {
        let c = camera.get_untracked();
        record.set(c.recording.stream);
        motion.set(c.motion.stream);
    });
    save::follow_server(move || (record.get(), motion.get()), saved, on_revert);

    let no_sub = "No substream configured";
    let sub = |label, desc| {
        let c = Choice::new(StreamRole::Sub, label).describe(desc);
        if has_sub { c } else { c.disabled_because(no_sub) }
    };
    let record_opts = vec![
        Choice::new(StreamRole::Main, "Use main stream for recording").tag("Recommended").describe("Full quality clips."),
        sub("Use substream for recording", "Smaller files, lower quality."),
    ];
    let motion_opts = vec![
        Choice::new(StreamRole::Main, "Use main stream for motion detection").describe("Most accurate; more CPU for software detection."),
        sub("Use substream for motion detection", "Much lighter on the NAS. Recommended when a substream exists."),
    ];
    let edit = format!("/cameras/{}/edit", cam.id);

    view! {
        <div class="settings-tab">
            <div class="stream-cards">
                {move || view! { <StreamCard title="Main stream" stream=Some(camera.get().main_stream) edit=edit.clone() /> }}
                {move || view! { <StreamCard title="Substream" stream=camera.get().sub_stream edit=format!("/cameras/{}/edit", camera.get().id) /> }}
            </div>
            <div class="form-grid">
                <FormSection title="Recording stream"><RadioCards value=record options=record_opts name="record-stream" /></FormSection>
                <FormSection title="Motion detection stream"><RadioCards value=motion options=motion_opts name="motion-stream" /></FormSection>
            </div>
            <SaveBar state dirty on_save on_revert />
        </div>
    }
}

#[component]
fn StreamCard(title: &'static str, stream: Option<Stream>, edit: String) -> impl IntoView {
    let Some(s) = stream else {
        return view! {
            <section class="stream-card stream-card--empty">
                <h3 class="stream-card__title">{title}</h3>
                <p class="muted">"Not configured. Watchgrid uses the main stream for everything — this is fine."</p>
                <A href=edit attr:class="btn btn--secondary btn--sm">"Add substream"</A>
            </section>
        }
        .into_any();
    };
    let (tone, label) = match s.status {
        StreamStatus::Active => (Tone::Stream, "ACTIVE"),
        StreamStatus::Idle => (Tone::Offline, "IDLE"),
        StreamStatus::Unconfigured => (Tone::Offline, "NOT SET"),
        StreamStatus::Error => (Tone::Danger, "ERROR"),
    };
    let dash = || "—".to_string();
    let rows = vec![
        ("Codec", s.codec.clone().unwrap_or_else(dash)),
        ("Resolution", s.width.zip(s.height).map(|(w, h)| format!("{w}×{h}")).unwrap_or_else(dash)),
        ("FPS", s.fps.map(|f| format!("{f:.0}")).unwrap_or_else(dash)),
        ("Bitrate", s.bitrate.map(|b| format!("{b} kbit/s")).unwrap_or_else(dash)),
        ("Audio", s.audio_codec.clone().unwrap_or_else(|| "None".into())),
    ];
    view! {
        <section class="stream-card">
            <header class="stream-card__head">
                <h3 class="stream-card__title">{title}</h3>
                <Badge tone label dot=true />
            </header>
            <div class="stream-card__url mono">{s.url}</div>
            <dl class="kv">
                {rows.into_iter().map(|(k, v)| view! { <div><dt>{k}</dt><dd>{v}</dd></div> }).collect_view()}
            </dl>
        </section>
    }
    .into_any()
}
