//! Recent server log with level filter, search, follow and pause.

use std::time::Duration;

use leptos::html::Div;
use leptos::prelude::*;

use crate::api::{self, LogEntry, LogLevel, LogQuery, Topic, use_query};
use crate::ui::form::Segmented;
use crate::ui::{I, Icon};

const LIMIT: u32 = 300;

fn level_name(l: LogLevel) -> &'static str {
    match l {
        LogLevel::Debug => "DEBUG",
        LogLevel::Info => "INFO",
        LogLevel::Warn => "WARN",
        LogLevel::Error => "ERROR",
    }
}

#[component]
pub fn LogViewer() -> impl IntoView {
    let min = RwSignal::new(LogLevel::Info);
    let search = RwSignal::new(String::new());
    let follow = RwSignal::new(true);
    let paused = RwSignal::new(false);
    let logs = use_query(Topic::System, Some(Duration::from_secs(2)), move || {
        api::get_logs(LogQuery { min_level: Some(min.get()), search: Some(search.get()).filter(|s| !s.is_empty()), limit: Some(LIMIT) })
    });

    // While paused, keep showing the last snapshot.
    let shown = RwSignal::new(Vec::<LogEntry>::new());
    Effect::new(move || {
        if let Some(Ok(list)) = logs.get()
            && !paused.get_untracked()
        {
            shown.set(list);
        }
    });

    let list_ref = NodeRef::<Div>::new();
    Effect::new(move || {
        shown.track();
        if follow.get_untracked()
            && let Some(el) = list_ref.get_untracked()
        {
            request_animation_frame(move || el.set_scroll_top(el.scroll_height()));
        }
    });

    let levels = vec![
        (LogLevel::Debug, view! { <span>"All"</span> }.into_any()),
        (LogLevel::Info, view! { <span>"Info+"</span> }.into_any()),
        (LogLevel::Warn, view! { <span>"Warnings"</span> }.into_any()),
        (LogLevel::Error, view! { <span>"Errors"</span> }.into_any()),
    ];

    view! {
        <div class="logs">
            <div class="logs__toolbar">
                <Segmented value=min options=levels label="Minimum level" />
                <label class="search search--sm">
                    <Icon icon=I::Search class="icon icon--sm" />
                    <input class="search__input" type="search" placeholder="Filter by text or source" bind:value=search />
                </label>
                <span class="toolbar__spacer"></span>
                <button class="toggle-chip" class:toggle-chip--on=follow aria-pressed=move || follow.get().to_string()
                    on:click=move |_| follow.update(|f| *f = !*f)>"Follow"</button>
                <button class="btn btn--secondary btn--sm" aria-pressed=move || paused.get().to_string() on:click=move |_| paused.update(|p| *p = !*p)>
                    {move || view! { <Icon icon=if paused.get() { I::Play } else { I::Pause } class="icon icon--sm" /> }}
                    {move || if paused.get() { "Resume" } else { "Pause" }}
                </button>
            </div>
            <div class="logs__list" node_ref=list_ref role="log" aria-live="polite">
                {move || {
                    let list = shown.get();
                    if list.is_empty() {
                        return view! { <p class="logs__empty">"No log lines match."</p> }.into_any();
                    }
                    list.into_iter().map(|l| {
                        let time = l.time.with_timezone(&chrono::Local).format("%H:%M:%S").to_string();
                        let lvl = level_name(l.level);
                        view! {
                            <div class=format!("log log--{}", lvl.to_lowercase())>
                                <span class="log__time">{time}</span>
                                <span class="log__level">{lvl}</span>
                                <span class="log__source">{l.source}</span>
                                <span class="log__msg">{l.message}</span>
                            </div>
                        }
                    }).collect_view().into_any()
                }}
            </div>
        </div>
    }
}
