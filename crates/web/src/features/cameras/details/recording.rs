use leptos::prelude::*;

use super::save;
use crate::api::{Camera, RecordingMode};
use crate::ui::{SaveBar, SaveState};
use crate::ui::form::{Choice, Field, FormSection, NumberInput, RadioCards};

#[component]
pub fn RecordingTab(#[prop(into)] camera: Signal<Camera>) -> impl IntoView {
    let initial = camera.get_untracked().recording;
    let mode = RwSignal::new(initial.mode);
    let pre = RwSignal::new(initial.pre_record_seconds);
    let post = RwSignal::new(initial.post_record_seconds);
    let min_event = RwSignal::new(initial.min_event_seconds);
    let max_clip = RwSignal::new(initial.max_clip_seconds);
    let merge = RwSignal::new(initial.event_merge_seconds);
    let state = SaveState::new();

    let current = move || (mode.get(), pre.get(), post.get(), min_event.get(), max_clip.get(), merge.get());
    let saved = move || {
        let r = camera.get().recording;
        (r.mode, r.pre_record_seconds, r.post_record_seconds, r.min_event_seconds, r.max_clip_seconds, r.event_merge_seconds)
    };
    let dirty = Signal::derive(move || current() != saved());
    let event_based = move || mode.get() == RecordingMode::Events;

    let on_save = Callback::new(move |_| {
        let (m, a, b, c, d, e) = current();
        save::camera(state, &camera.get_untracked(), |i| {
            i.recording.mode = m;
            i.recording.pre_record_seconds = a;
            i.recording.post_record_seconds = b;
            i.recording.min_event_seconds = c;
            i.recording.max_clip_seconds = d;
            i.recording.event_merge_seconds = e;
        });
    });
    let on_revert = Callback::new(move |_| {
        let r = camera.get_untracked().recording;
        mode.set(r.mode);
        pre.set(r.pre_record_seconds);
        post.set(r.post_record_seconds);
        min_event.set(r.min_event_seconds);
        max_clip.set(r.max_clip_seconds);
        merge.set(r.event_merge_seconds);
    });

    let modes = vec![
        Choice::new(RecordingMode::Disabled, "Disabled").describe("Never record. Live view keeps working."),
        Choice::new(RecordingMode::Events, "Events / Motion").tag("Default")
            .describe("Record only when something happens, with pre- and post-record buffers."),
        Choice::new(RecordingMode::Manual, "Manual only").describe("Record only when you press Record (or via the API)."),
        Choice::new(RecordingMode::Continuous, "Continuous").describe("Record around the clock. Uses much more storage."),
        Choice::new(RecordingMode::Scheduled, "Scheduled").tag("Future").disabled_because("Schedules arrive in a later version"),
    ];

    view! {
        <div class="settings-tab">
            <FormSection title="Recording mode" description="Manual recording from the Record button always works, whatever the mode.">
                <RadioCards value=mode options=modes name="recording-mode" />
            </FormSection>

            <FormSection title="Event recording" description="How clips are cut around each event.">
                <BufferDiagram pre post />
                <div class="form-grid form-grid--3">
                    <Field label="Pre-record" hint="Video kept from before the event started">
                        <NumberInput value=pre min=0 max=60 suffix="seconds" disabled=Signal::derive(move || !event_based()) />
                    </Field>
                    <Field label="Post-record" hint="Keep recording after the event ends">
                        <NumberInput value=post min=0 max=300 suffix="seconds" disabled=Signal::derive(move || !event_based()) />
                    </Field>
                    <Field label="Minimum event duration" hint="Shorter events are ignored">
                        <NumberInput value=min_event min=0 max=60 suffix="seconds" disabled=Signal::derive(move || !event_based()) />
                    </Field>
                    <Field label="Maximum clip duration" hint="Longer events are split into several clips">
                        <NumberInput value=max_clip min=30 max=3600 suffix="seconds" />
                    </Field>
                    <Field label="Event merging interval" hint="Motion restarting within this gap continues the same clip">
                        <NumberInput value=merge min=0 max=120 suffix="seconds" disabled=Signal::derive(move || !event_based()) />
                    </Field>
                </div>
                <p class="note">
                    {move || format!(
                        "Example: if motion stops and starts again within {} s, Watchgrid keeps the same recording instead of creating another clip.",
                        merge.get()
                    )}
                </p>
            </FormSection>

            <SaveBar state dirty on_save on_revert />
        </div>
    }
}

/// [ pre ][ EVENT ][ post ] illustration, proportional to the values.
#[component]
fn BufferDiagram(pre: RwSignal<u32>, post: RwSignal<u32>) -> impl IntoView {
    const EVENT: u32 = 20;
    let total = move || (pre.get() + EVENT + post.get()).max(1) as f32;
    let pct = move |v: u32| format!("{:.1}%", v as f32 / total() * 100.0);
    view! {
        <div class="buffer-diagram" aria-hidden="true">
            <div class="buffer-diagram__pre" style:width=move || pct(pre.get())>{move || format!("{} s pre-record", pre.get())}</div>
            <div class="buffer-diagram__event" style:width=move || pct(EVENT)>"EVENT"</div>
            <div class="buffer-diagram__post" style:width=move || pct(post.get())>{move || format!("{} s post-record", post.get())}</div>
        </div>
    }
}
