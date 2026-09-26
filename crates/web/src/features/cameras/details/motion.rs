use leptos::prelude::*;

use super::save;
use super::zones::ZoneEditor;
use crate::api::{Camera, MotionSource};
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
        .describe("The camera reports motion, people and vehicles itself. Lightest on the NAS.");
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
