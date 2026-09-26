//! DVR-style playback of one camera's day: starts at the moment clicked on
//! the timeline and moves from clip to clip by itself, skipping the gaps.

use chrono::{DateTime, Utc};
use leptos::html::Video;
use leptos::prelude::*;

use crate::api::{self, Recording};
use crate::format;
use crate::ui::{I, Icon};

/// Where to start: the clip covering `at` (and how far into it), else the
/// next clip after it.
fn start_point(clips: &[Recording], at: DateTime<Utc>) -> Option<(usize, f64)> {
    let end = |r: &Recording| r.end_time.unwrap_or_else(Utc::now);
    if let Some(i) = clips.iter().position(|r| r.start_time <= at && at < end(r)) {
        return Some((i, (at - clips[i].start_time).num_milliseconds() as f64 / 1000.0));
    }
    clips.iter().position(|r| r.start_time > at).map(|i| (i, 0.0))
}

#[component]
pub fn DvrPlayer(
    camera_name: String,
    /// One camera's recordings of the day, oldest first.
    clips: Vec<Recording>,
    at: DateTime<Utc>,
    /// Updated with the wall-clock time of the picture (timeline marker).
    playhead: RwSignal<Option<DateTime<Utc>>>,
    on_details: Callback<Recording>,
    on_close: Callback<()>,
) -> impl IntoView {
    let video = NodeRef::<Video>::new();
    let start = start_point(&clips, at);
    let clips = StoredValue::new(clips);
    let index = RwSignal::new(start.map(|(i, _)| i));
    // Seconds to jump to once the clip has loaded.
    let pending = StoredValue::new(start.map_or(0.0, |(_, o)| o));
    // Bumped to fetch a longer copy of a clip still being recorded.
    let reloads = RwSignal::new(0u32);
    let note = RwSignal::new(None::<String>);
    let clip = move || index.get().and_then(|i| clips.with_value(|c| c.get(i).cloned()));
    let count = clips.with_value(Vec::len);

    let src = move || {
        let r = clip()?;
        let url = api::recording_media_url(&r.id)?;
        let n = reloads.get();
        Some(if n == 0 { url } else { format!("{url}?at={n}") })
    };
    let go = move |i: usize| {
        pending.set_value(0.0);
        reloads.set(0);
        index.set(Some(i));
    };
    let on_loaded = move |_| {
        let Some(v) = video.get_untracked() else { return };
        let t = pending.get_value();
        if t > 0.0 {
            v.set_current_time(t);
            pending.set_value(0.0);
        }
        let _ = v.play();
    };
    let on_time = move |_| {
        if let (Some(v), Some(r)) = (video.get_untracked(), clip()) {
            playhead.set(Some(r.start_time + chrono::Duration::milliseconds((v.current_time() * 1000.0) as i64)));
        }
    };
    let on_ended = move |_| {
        let (Some(i), Some(v)) = (index.get_untracked(), video.get_untracked()) else { return };
        let current = clips.with_value(|c| c[i].clone());
        if current.end_time.is_none() {
            // Still recording: fetch what was added and continue.
            pending.set_value(v.current_time());
            reloads.update(|n| *n += 1);
            return;
        }
        match clips.with_value(|c| c.get(i + 1).cloned()) {
            Some(next) => {
                let gap = (next.start_time - current.end_time.unwrap_or(next.start_time)).num_seconds();
                note.set((gap > 5).then(|| format!("Skipped {} without recording", format::duration(gap as u32))));
                go(i + 1);
            }
            None => note.set(Some("End of this day's recordings".into())),
        }
    };
    on_cleanup(move || playhead.set(None));

    let clock = move || playhead.get().map(|t| format::date_time(t.with_timezone(&chrono::Local))).unwrap_or_default();
    let position = move || index.get().map(|i| format!("Clip {} of {count}", i + 1)).unwrap_or_default();

    view! {
        <section class="dvr" aria-label="Playback">
            <header class="dvr__head">
                <strong>{camera_name}</strong>
                <span class="dvr__clock mono">{clock}</span>
                <span class="dvr__spacer"></span>
                <button class="icon-btn" aria-label="Close" title="Close" on:click=move |_| on_close.run(())><Icon icon=I::X /></button>
            </header>
            {move || match (index.get(), src()) {
                (None, _) => view! { <p class="dvr__empty">"No recording from this moment on today."</p> }.into_any(),
                (Some(_), None) => view! { <p class="dvr__empty">"The demo data has no video files."</p> }.into_any(),
                (Some(_), Some(url)) => view! {
                    <video class="dvr__video" node_ref=video src=url controls autoplay playsinline
                        on:loadedmetadata=on_loaded on:timeupdate=on_time on:ended=on_ended></video>
                }.into_any(),
            }}
            <div class="dvr__controls">
                <button class="btn btn--secondary btn--sm" disabled=move || index.get().is_none_or(|i| i == 0)
                    on:click=move |_| if let Some(i) = index.get_untracked().filter(|i| *i > 0) { note.set(None); go(i - 1) }>
                    <Icon icon=I::ChevronLeft class="icon icon--sm" />"Previous clip"
                </button>
                <button class="btn btn--secondary btn--sm" disabled=move || index.get().is_none_or(|i| i + 1 >= count)
                    on:click=move |_| if let Some(i) = index.get_untracked().filter(|i| i + 1 < count) { note.set(None); go(i + 1) }>
                    "Next clip"<Icon icon=I::ChevronRight class="icon icon--sm" />
                </button>
                <span class="dvr__position">{position}</span>
                {move || note.get().map(|n| view! { <span class="dvr__note">{n}</span> })}
                <span class="dvr__spacer"></span>
                <button class="btn btn--ghost btn--sm" disabled=move || clip().is_none()
                    on:click=move |_| if let Some(r) = clip() { on_details.run(r) }>
                    "Clip details"
                </button>
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::RecordingReason;

    fn clip(id: &str, start: &str, secs: i64) -> Recording {
        let s: DateTime<Utc> = start.parse().unwrap();
        Recording {
            id: id.into(),
            camera_id: "cam".into(),
            start_time: s,
            end_time: Some(s + chrono::Duration::seconds(secs)),
            duration: secs as u32,
            reason: RecordingReason::Continuous,
            file_size: 1,
            protected: false,
            event_ids: vec![],
        }
    }

    #[test]
    fn starts_inside_a_clip_or_at_the_next_one() {
        let clips = [clip("a", "2026-09-26T10:00:00Z", 600), clip("b", "2026-09-26T11:00:00Z", 600)];
        let t = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
        assert_eq!(start_point(&clips, t("2026-09-26T10:05:00Z")), Some((0, 300.0)));
        assert_eq!(start_point(&clips, t("2026-09-26T10:30:00Z")), Some((1, 0.0)), "in a gap: the next clip");
        assert_eq!(start_point(&clips, t("2026-09-26T12:00:00Z")), None, "after the last one");
    }
}
