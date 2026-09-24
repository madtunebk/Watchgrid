use leptos::prelude::*;

use super::save;
use crate::api::{Camera, MotionSource, MotionZone};
use crate::features::cameras::widgets::CameraPreview;
use crate::ui::{SaveBar, SaveState};
use crate::ui::form::{Choice, Field, FormSection, RadioCards, Slider, Switch};

#[component]
pub fn MotionTab(#[prop(into)] camera: Signal<Camera>) -> impl IntoView {
    let initial = camera.get_untracked().motion;
    let has_onvif = camera.get_untracked().onvif.is_some();
    let enabled = RwSignal::new(initial.enabled);
    let source = RwSignal::new(initial.source);
    let sensitivity = RwSignal::new(initial.sensitivity);
    let state = SaveState::new();

    let saved = move || {
        let m = camera.get().motion;
        (m.enabled, m.source, m.sensitivity)
    };
    let dirty = Signal::derive(move || (enabled.get(), source.get(), sensitivity.get()) != saved());
    let on_save = Callback::new(move |_| {
        let (e, s, v) = (enabled.get_untracked(), source.get_untracked(), sensitivity.get_untracked());
        save::camera(state, &camera.get_untracked(), |i| {
            i.motion.enabled = e;
            i.motion.source = s;
            i.motion.sensitivity = v;
        });
    });
    let on_revert = Callback::new(move |_| {
        let m = camera.get_untracked().motion;
        enabled.set(m.enabled);
        source.set(m.source);
        sensitivity.set(m.sensitivity);
    });

    let onvif = Choice::new(MotionSource::Onvif, "Camera / ONVIF").tag("Recommended")
        .describe("The camera reports motion, people and vehicles itself. Lightest on the NAS.");
    let sources = vec![
        if has_onvif { onvif } else { onvif.disabled_because("Add ONVIF details in Edit camera to use this") },
        Choice::new(MotionSource::Software, "Software detection")
            .describe("Watchgrid analyses the video. Works with any camera; uses some CPU."),
        Choice::new(MotionSource::Ai, "AI object detection").tag("Future")
            .disabled_because("Person / vehicle / animal detection arrives in a later version"),
    ];
    let off = Signal::derive(move || !enabled.get());

    view! {
        <div class="settings-tab">
            <FormSection title="Motion detection">
                <Switch checked=enabled label="Detect motion on this camera"
                    description="Detected motion creates events and, in Events mode, starts a recording." />
            </FormSection>

            <FormSection title="Detection source">
                <fieldset class="plain-fieldset" disabled=off>
                    <RadioCards value=source options=sources name="motion-source" />
                </fieldset>
            </FormSection>

            <div class="motion-layout">
                <FormSection title="Sensitivity" description="Higher values react to smaller movements.">
                    <Field label="Sensitivity">
                        <Slider value=sensitivity disabled=off />
                    </Field>
                </FormSection>
                <FormSection title="Detection zones" description="Only motion inside green zones triggers; red zones are ignored.">
                    <ZoneOverlay camera zones=camera.get_untracked().motion.zones />
                    <p class="note">"Drawing and editing zones arrives with the detection engine. The zones shown are read-only."</p>
                </FormSection>
            </div>

            <SaveBar state dirty on_save on_revert />
        </div>
    }
}

/// Camera preview with the configured zones drawn on top.
#[component]
fn ZoneOverlay(camera: Signal<Camera>, zones: Vec<MotionZone>) -> impl IntoView {
    let empty = zones.is_empty();
    view! {
        <div class="zone-stage">
            {move || view! { <CameraPreview camera=camera.get() substream=true /> }}
            <svg class="zone-stage__svg" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
                {zones.iter().map(|z| view! {
                    <rect x=z.x * 100.0 y=z.y * 100.0 width=z.w * 100.0 height=z.h * 100.0
                        class=if z.exclude { "zone zone--exclude" } else { "zone" } />
                }).collect_view()}
            </svg>
            <div class="zone-stage__labels">
                {zones.into_iter().map(|z| view! {
                    <span class="zone-label" class:zone-label--exclude=z.exclude
                        style:left=format!("{}%", z.x * 100.0) style:top=format!("{}%", (z.y + z.h) * 100.0)>
                        {z.name}
                    </span>
                }).collect_view()}
            </div>
            {empty.then(|| view! { <div class="zone-stage__empty">"Whole frame (no zones defined)"</div> })}
        </div>
    }
}
