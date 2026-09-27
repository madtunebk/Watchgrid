use leptos::prelude::*;

use super::save;
use super::zones::ZoneEditor;
use crate::api::{Camera, DetectorState, MotionSource};
use crate::ui::{SaveBar, SaveState};
use crate::ui::form::{Choice, Field, FormSection, RadioCards, Slider, Switch};

#[component]
pub fn MotionTab(#[prop(into)] camera: Signal<Camera>) -> impl IntoView {
    let initial = camera.get_untracked().motion;
    let has_onvif = camera.get_untracked().onvif.is_some();
    let enabled = RwSignal::new(initial.enabled);
    let source = RwSignal::new(initial.source);
    let sensitivity = RwSignal::new(initial.sensitivity);
    let zones = RwSignal::new(initial.zones);
    let notify = RwSignal::new(initial.notify);
    let state = SaveState::new();

    let saved = move || {
        let m = camera.get().motion;
        (m.enabled, m.source, m.sensitivity, m.zones, m.notify)
    };
    let dirty = Signal::derive(move || (enabled.get(), source.get(), sensitivity.get(), zones.get(), notify.get()) != saved());
    let on_save = Callback::new(move |_| {
        let (e, s, v, z, n) = (enabled.get_untracked(), source.get_untracked(), sensitivity.get_untracked(), zones.get_untracked(), notify.get_untracked());
        save::camera(state, &camera.get_untracked(), |i| {
            i.motion.enabled = e;
            i.motion.source = s;
            i.motion.sensitivity = v;
            i.motion.zones = z;
            i.motion.notify = n;
        });
    });
    let on_revert = Callback::new(move |_| {
        let m = camera.get_untracked().motion;
        enabled.set(m.enabled);
        source.set(m.source);
        sensitivity.set(m.sensitivity);
        zones.set(m.zones);
        notify.set(m.notify);
    });

    save::follow_server(move || (enabled.get(), source.get(), sensitivity.get(), zones.get(), notify.get()), saved, on_revert);

    let onvif = Choice::new(MotionSource::Onvif, "Camera / ONVIF").tag("Recommended")
        .describe("The camera reports motion, people and vehicles itself. Lightest on the NAS. If its events stop working, Watchgrid detects motion itself until they are back.");
    let sources = vec![
        if has_onvif { onvif } else { onvif.disabled_because("Add ONVIF details in Edit camera to use this") },
        Choice::new(MotionSource::Software, "Software detection")
            .describe("Watchgrid analyses the substream itself. For cameras without ONVIF events; motion only, a little CPU."),
        Choice::new(MotionSource::Ai, "AI object detection").tag("Future")
            .disabled_because("Person / vehicle / animal detection arrives in a later version"),
    ];
    let off = Signal::derive(move || !enabled.get());

    view! {
        <div class="settings-tab">
            <FormSection title="Motion detection">
                <Switch checked=enabled label="Detect motion on this camera"
                    description="Detected motion creates events and, in Events mode, starts a recording." />
                <fieldset class="plain-fieldset" disabled=off>
                    <Switch checked=notify label="Notify me on motion"
                        description="A notification in the bell (and the webhook) when this camera sees motion, at most one every 5 minutes. People and vehicles follow Settings → Notifications." />
                </fieldset>
            </FormSection>

            <FormSection title="Detection source">
                <fieldset class="plain-fieldset" disabled=off>
                    <RadioCards value=source options=sources name="motion-source" />
                </fieldset>
                // The camera's own events: connected proves delivery; the
                // last reported detection proves motion gets through.
                {move || {
                    let c = camera.get();
                    (source.get() == MotionSource::Onvif).then_some(()).and(c.onvif_events).map(|e| {
                        let text = match (e.connected, e.last_detection) {
                            (false, _) => "Camera events: not connected right now (Watchgrid keeps retrying).".to_string(),
                            (true, Some(t)) => format!("Camera events: connected · last detection reported {}.", crate::format::relative(t)),
                            (true, None) => "Camera events: connected · no detection reported yet (move in front of the camera to confirm).".to_string(),
                        };
                        view! { <p class=if e.connected { "note" } else { "note note--warning" }>{text}</p> }
                    })
                }}
                // What detection is really doing right now (not what was chosen).
                {move || {
                    let c = camera.get();
                    let stand_in = c.motion_fallback && source.get() == MotionSource::Onvif;
                    let why = c.software_motion.as_ref().and_then(|s| s.detail.clone()).unwrap_or_default();
                    match (c.software_motion.map(|s| s.state), stand_in) {
                        (Some(DetectorState::Failing), true) => view! {
                            <p class="note note--danger">
                                <strong>"No motion detection on this camera right now."</strong>
                                {format!(" Its ONVIF events don't work, and Watchgrid can't detect motion itself either: {why}.")}
                            </p>
                        }.into_any(),
                        (Some(DetectorState::Failing), false) => view! {
                            <p class="note note--danger"><strong>"Software detection isn't working: "</strong>{why}"."</p>
                        }.into_any(),
                        (Some(DetectorState::Working), true) => view! {
                            <p class="note note--warning">
                                <strong>"The camera's motion events aren't working right now"</strong>
                                " (its ONVIF connection fails or keeps dropping). Watchgrid is detecting motion itself on the substream until they work again, "
                                "with the sensitivity and zones below. Restarting the camera usually brings its events back."
                            </p>
                        }.into_any(),
                        (Some(DetectorState::Starting), true) => view! {
                            <p class="note note--warning">
                                <strong>"The camera's motion events aren't working right now."</strong>
                                " Watchgrid is starting its own detection on the substream…"
                            </p>
                        }.into_any(),
                        (Some(DetectorState::Working), false) => view! { <p class="note note--ok">"Software detection is working: pictures are analysed."</p> }.into_any(),
                        (Some(DetectorState::Starting), false) => view! { <p class="note">"Software detection is starting (waiting for video)…"</p> }.into_any(),
                        (None, _) => ().into_any(),
                    }
                }}
            </FormSection>

            <div class="motion-layout">
                <FormSection title="Sensitivity" description="Higher values react to smaller movements.">
                    <Field label="Sensitivity">
                        <Slider value=sensitivity disabled=off />
                    </Field>
                </FormSection>
                <FormSection title="Detection zones" description="Drag on the picture to draw a zone; drag a zone to move it. Green zones limit detection to them, red zones are ignored.">
                    <ZoneEditor camera zones disabled=off />
                    {move || (source.get() == MotionSource::Onvif).then(|| view! {
                        <p class="note">"Zones apply to software detection. With ONVIF, set the detection area in the camera's own settings."</p>
                    })}
                </FormSection>
            </div>

            <SaveBar state dirty on_save on_revert />
        </div>
    }
}
