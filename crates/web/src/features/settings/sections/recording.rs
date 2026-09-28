use leptos::prelude::*;

use crate::api::{RecordingMode, Settings};
use crate::features::settings::save::save;
use crate::ui::form::{Choice, Field, FormSection, NumberInput, RadioCards};
use crate::ui::{follow_server, SaveBar, SaveState};

/// Defaults for newly added cameras.
#[component]
pub fn RecordingSection(settings: Signal<Settings>) -> impl IntoView {
    let r = settings.get_untracked().recording;
    let mode = RwSignal::new(r.mode);
    let pre = RwSignal::new(r.pre_record_seconds);
    let post = RwSignal::new(r.post_record_seconds);
    let state = SaveState::new();

    let dirty = Signal::derive(move || {
        let r = settings.get().recording;
        (mode.get(), pre.get(), post.get()) != (r.mode, r.pre_record_seconds, r.post_record_seconds)
    });
    let on_save = Callback::new(move |_| {
        let (m, a, b) = (mode.get_untracked(), pre.get_untracked(), post.get_untracked());
        save(state, &settings.get_untracked(), move |s| {
            s.recording.mode = m;
            s.recording.pre_record_seconds = a;
            s.recording.post_record_seconds = b;
        });
    });
    let on_revert = Callback::new(move |_| {
        let r = settings.get_untracked().recording;
        mode.set(r.mode);
        pre.set(r.pre_record_seconds);
        post.set(r.post_record_seconds);
    });
    follow_server(move || (mode.get(), pre.get(), post.get()), move || { let r = settings.get().recording; (r.mode, r.pre_record_seconds, r.post_record_seconds) }, on_revert);
    let modes = vec![
        Choice::new(RecordingMode::Events, "Events / Motion").tag("Recommended").describe("Record when something happens."),
        Choice::new(RecordingMode::Manual, "Manual only").describe("Record only when started by hand or API."),
        Choice::new(RecordingMode::Continuous, "Continuous").describe("Record around the clock."),
        Choice::new(RecordingMode::Disabled, "Disabled").describe("Live view only."),
    ];

    view! {
        <div class="settings-tab">
            <FormSection title="Defaults for new cameras" description="Each camera keeps its own settings; changing these does not alter existing cameras.">
                <Field label="Recording mode"><RadioCards value=mode options=modes name="default-mode" /></Field>
                <div class="form-grid">
                    <Field label="Pre-record" hint="Seconds kept from before an event, when recording the substream (up to 30)">
                        <NumberInput value=pre min=0 max=30 suffix="seconds" />
                    </Field>
                    <Field label="Post-record" hint="Seconds recorded after an event ends (up to 300)">
                        <NumberInput value=post min=0 max=300 suffix="seconds" />
                    </Field>
                </div>
            </FormSection>
            <SaveBar state dirty on_save on_revert />
        </div>
    }
}
