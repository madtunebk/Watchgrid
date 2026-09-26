use leptos::prelude::*;

use super::save;
use crate::api::{Camera, MotionSource, RecordingMode, ScheduleWindow};
use crate::ui::{SaveBar, SaveState};
use crate::ui::form::{Choice, Field, FormSection, NumberInput, RadioCards, Switch};

#[component]
pub fn RecordingTab(#[prop(into)] camera: Signal<Camera>) -> impl IntoView {
    let initial = camera.get_untracked().recording;
    let mode = RwSignal::new(initial.mode);
    let pre = RwSignal::new(initial.pre_record_seconds);
    let post = RwSignal::new(initial.post_record_seconds);
    let min_event = RwSignal::new(initial.min_event_seconds);
    let max_clip = RwSignal::new(initial.max_clip_seconds);
    let merge = RwSignal::new(initial.event_merge_seconds);
    let schedule = RwSignal::new(initial.schedule.clone());
    let sound = RwSignal::new(initial.record_audio);
    let state = SaveState::new();

    let current = move || (mode.get(), pre.get(), post.get(), min_event.get(), max_clip.get(), merge.get(), schedule.get(), sound.get());
    let saved = move || {
        let r = camera.get().recording;
        (r.mode, r.pre_record_seconds, r.post_record_seconds, r.min_event_seconds, r.max_clip_seconds, r.event_merge_seconds, r.schedule, r.record_audio)
    };
    let dirty = Signal::derive(move || current() != saved());
    let event_based = move || mode.get() == RecordingMode::Events;
    // Event recording needs something that reports events: the camera's own
    // ONVIF motion or Watchgrid's software detection.
    let has_detection = move || {
        let c = camera.get();
        c.motion.enabled
            && match c.motion.source {
                MotionSource::Onvif => c.onvif.as_ref().is_some_and(|o| !o.url.trim().is_empty()),
                MotionSource::Software => true,
                MotionSource::Ai => false,
            }
    };

    let on_save = Callback::new(move |_| {
        let (m, a, b, c, d, e, windows, with_sound) = current();
        save::camera(state, &camera.get_untracked(), |i| {
            i.recording.mode = m;
            i.recording.pre_record_seconds = a;
            i.recording.post_record_seconds = b;
            i.recording.min_event_seconds = c;
            i.recording.max_clip_seconds = d;
            i.recording.event_merge_seconds = e;
            i.recording.schedule = windows;
            i.recording.record_audio = with_sound;
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
        schedule.set(r.schedule);
        sound.set(r.record_audio);
    });
    save::follow_server(current, saved, on_revert);

    let modes = vec![
        Choice::new(RecordingMode::Disabled, "Disabled").describe("Never record. Live view keeps working."),
        Choice::new(RecordingMode::Events, "Events / Motion").tag("Default")
            .describe("Record only when something happens, with pre- and post-record buffers."),
        Choice::new(RecordingMode::Manual, "Manual only").describe("Record only when you press Record (or via the API)."),
        Choice::new(RecordingMode::Continuous, "Continuous").describe("Record around the clock. Uses much more storage."),
        Choice::new(RecordingMode::Scheduled, "Scheduled").describe("Record continuously inside the weekly time windows below."),
    ];

    view! {
        <div class="settings-tab">
            <FormSection title="Recording mode" description="Manual recording from the Record button always works, whatever the mode.">
                <RadioCards value=mode options=modes name="recording-mode" />
                <Show when=move || event_based() && !has_detection()>
                    <p class="note note--warn">
                        "This camera has no motion source yet, so in Events mode it will never record by itself. "
                        "Turn on motion detection in the Motion tab: ONVIF events if the camera supports them, "
                        "otherwise Software detection."
                    </p>
                </Show>
            </FormSection>

            <Show when=move || mode.get() == RecordingMode::Scheduled>
                <FormSection title="Schedule" description="Times use the server's time zone (Settings → General). A window ending before it starts runs past midnight.">
                    <ScheduleEditor windows=schedule />
                </FormSection>
            </Show>
            <Show when=move || matches!(mode.get(), RecordingMode::Continuous | RecordingMode::Scheduled)>
                <p class="note">{move || format!("Recordings are cut into clips of {} minutes; each clip continues where the previous one ended.", max_clip.get() / 60)}</p>
            </Show>

            <FormSection title="Sound">
                <Switch checked=sound label="Record sound"
                    description="Saved with the video when the camera has a microphone. Recordings of the substream have no sound." />
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

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn hhmm(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

fn parse_hhmm(s: &str) -> Option<u16> {
    let (h, m) = s.split_once(':')?;
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Weekly windows: day toggles plus start/end times per window.
#[component]
fn ScheduleEditor(windows: RwSignal<Vec<ScheduleWindow>>) -> impl IntoView {
    let add = move |_| windows.update(|w| w.push(ScheduleWindow { days: vec![0, 1, 2, 3, 4], start_minute: 22 * 60, end_minute: 6 * 60 }));
    view! {
        <div class="schedule">
            {move || {
                let list = windows.get();
                if list.is_empty() {
                    return view! { <p class="note">"No windows yet — nothing will be recorded until you add one."</p> }.into_any();
                }
                list.into_iter().enumerate().map(|(i, w)| {
                    let set_time = move |start: bool, value: String| {
                        if let Some(m) = parse_hhmm(&value) {
                            windows.update(|all| if start { all[i].start_minute = m } else { all[i].end_minute = m });
                        }
                    };
                    view! {
                        <div class="schedule__row">
                            <div class="schedule__days" role="group" aria-label="Days">
                                {DAYS.iter().enumerate().map(|(d, label)| {
                                    let d = d as u8;
                                    let on = w.days.contains(&d);
                                    view! {
                                        <button type="button" class="schedule__day" class:schedule__day--on=on aria-pressed=on.to_string()
                                            on:click=move |_| windows.update(|all| {
                                                let days = &mut all[i].days;
                                                if let Some(p) = days.iter().position(|x| *x == d) { days.remove(p); } else { days.push(d); days.sort_unstable(); }
                                            })>{*label}</button>
                                    }
                                }).collect_view()}
                            </div>
                            <input class="input schedule__time" type="time" aria-label="From" prop:value=hhmm(w.start_minute)
                                on:change=move |ev| set_time(true, event_target_value(&ev)) />
                            <span class="muted">"–"</span>
                            <input class="input schedule__time" type="time" aria-label="Until" prop:value=hhmm(w.end_minute)
                                on:change=move |ev| set_time(false, event_target_value(&ev)) />
                            <button type="button" class="icon-btn" aria-label="Remove window" title="Remove window"
                                on:click=move |_| windows.update(|all| { all.remove(i); })>
                                <crate::ui::Icon icon=crate::ui::I::X class="icon icon--sm" />
                            </button>
                        </div>
                    }
                }).collect_view().into_any()
            }}
            <button type="button" class="btn btn--secondary btn--sm" on:click=add>
                <crate::ui::Icon icon=crate::ui::I::Plus class="icon icon--sm" />"Add window"
            </button>
        </div>
    }
}
