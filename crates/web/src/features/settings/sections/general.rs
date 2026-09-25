use leptos::prelude::*;

use crate::api::{DateFormat, Settings};
use crate::features::settings::save::save;
use crate::ui::form::{Choice, Field, FormSection, RadioCards, Switch, TextInput};
use crate::ui::{SaveBar, SaveState};

const ZONES: &[&str] = &[
    "UTC", "Europe/Bucharest", "Europe/London", "Europe/Berlin", "Europe/Paris", "Europe/Madrid", "Europe/Rome",
    "Europe/Athens", "Europe/Kyiv", "Europe/Moscow", "America/New_York", "America/Chicago", "America/Denver",
    "America/Los_Angeles", "America/Sao_Paulo", "Asia/Dubai", "Asia/Kolkata", "Asia/Shanghai", "Asia/Tokyo", "Australia/Sydney",
];
#[component]
pub fn GeneralSection(settings: Signal<Settings>) -> impl IntoView {
    let g = settings.get_untracked().general;
    let name = RwSignal::new(g.nvr_name);
    let zone = RwSignal::new(g.timezone);
    let date_format = RwSignal::new(g.date_format);
    let clock_24h = RwSignal::new(g.clock_24h);
    let state = SaveState::new();

    let draft = move || (name.get(), zone.get(), date_format.get(), clock_24h.get());
    let dirty = Signal::derive(move || {
        let g = settings.get().general;
        draft() != (g.nvr_name, g.timezone, g.date_format, g.clock_24h)
    });
    let on_save = Callback::new(move |_| {
        let (n, z, d, c) = draft();
        save(state, &settings.get_untracked(), |s| {
            s.general.nvr_name = n.trim().into();
            s.general.timezone = z;
            s.general.date_format = d;
            s.general.clock_24h = c;
        });
    });
    let on_revert = Callback::new(move |_| {
        let g = settings.get_untracked().general;
        name.set(g.nvr_name);
        zone.set(g.timezone);
        date_format.set(g.date_format);
        clock_24h.set(g.clock_24h);
    });
    let preview = move || {
        let now = chrono::Local::now();
        let date = match date_format.get() {
            DateFormat::Iso => now.format("%Y-%m-%d"),
            DateFormat::DayFirst => now.format("%d/%m/%Y"),
            DateFormat::MonthFirst => now.format("%m/%d/%Y"),
        };
        let time = if clock_24h.get() { now.format("%H:%M") } else { now.format("%-I:%M %p") };
        format!("{date} {time}")
    };
    let formats = vec![
        Choice::new(DateFormat::Iso, "2026-09-24").describe("ISO 8601 — sorts correctly everywhere").tag("Default"),
        Choice::new(DateFormat::DayFirst, "24/09/2026").describe("Day first"),
        Choice::new(DateFormat::MonthFirst, "09/24/2026").describe("Month first"),
    ];

    view! {
        <div class="settings-tab">
            <FormSection title="Identity">
                <Field label="NVR name" hint="Shown in the sidebar, browser title and notifications.">
                    <TextInput value=name placeholder="Home NVR" />
                </Field>
            </FormSection>
            <FormSection title="Region">
                <Field label="Time zone" hint="Used for schedules, day boundaries and timestamps in clips.">
                    <select class="select" on:change=move |ev| zone.set(event_target_value(&ev))>
                        {ZONES.iter().map(|z| view! { <option value=*z selected=move || zone.get() == *z>{*z}</option> }).collect_view()}
                    </select>
                </Field>
                <Field label="Date format"><RadioCards value=date_format options=formats name="date-format" /></Field>
                <Switch checked=clock_24h label="24-hour clock" />
                <p class="note">"Preview: " <strong class="mono">{preview}</strong></p>
            </FormSection>
            <SaveBar state dirty on_save on_revert />
        </div>
    }
}
