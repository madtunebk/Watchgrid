use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use leptos::ev;
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_query_map};
use leptos_router::NavigateOptions;

use super::bulk_bar::BulkBar;
use super::clips::ClipList;
use super::drawer::RecordingDrawer;
use super::dvr::DvrPlayer;
use super::state::{State, View, ZOOMS};
use super::timeline::{Scale, Timeline};
use super::toolbar::Toolbar;
use crate::api::{self, EventQuery, EventType, Recording, RecordingQuery, Topic, use_query};
use crate::features::cameras::NoCameras;
use crate::format;
use crate::ui::{EmptyState, ErrorBox, I, Page, Pager, Skeleton, keep_only_shown};

const CLIPS_PAGE: u32 = 100;
/// Events drawn on the timeline: what cameras see, and failed sign-ins.
const MARKED: [EventType; 6] = [EventType::Motion, EventType::Person, EventType::Vehicle, EventType::Animal, EventType::Onvif, EventType::Security];

#[component]
pub fn RecordingsPage() -> impl IntoView {
    let query = use_query_map();
    let state = Memo::new(move |_| query.with(State::from_query));
    let navigate = use_navigate();
    let on_change = Callback::new(move |s: State| navigate(&s.to_url(), NavigateOptions { replace: true, ..Default::default() }));

    let cameras = use_query(Topic::Cameras, None, api::get_cameras);
    let camera_list = Signal::derive(move || cameras.get().and_then(Result::ok).unwrap_or_default());
    let recordings = use_query(Topic::Recordings, Some(Duration::from_secs(30)), move || {
        let s = state.get();
        let (from, to) = s.day_range();
        api::get_recordings(RecordingQuery { camera_ids: s.cameras, from: Some(from), to: Some(to) })
    });
    // The day's detections, drawn as dots on the timeline rows.
    let events = use_query(Topic::Events, Some(Duration::from_secs(30)), move || {
        let (from, to) = state.get().day_range();
        api::get_events(EventQuery { from: Some(from), to: Some(to), kinds: MARKED.to_vec(), limit: Some(1000), ..Default::default() })
    });
    // Clips ticked for a bulk action; another day, camera or view starts over.
    let ticked = RwSignal::new(BTreeSet::<String>::new());
    // Clips list: pages of CLIPS_PAGE, newest first.
    let clip_page = RwSignal::new(0u32);
    Effect::new(move || {
        state.track();
        ticked.set(BTreeSet::new());
        clip_page.set(0);
    });
    let page_of = move |list: Vec<Recording>| -> Vec<Recording> {
        let skip = clip_page.get() as usize * CLIPS_PAGE as usize;
        let mut page: Vec<Recording> = list.into_iter().rev().skip(skip).take(CLIPS_PAGE as usize).collect();
        page.reverse(); // ClipList shows oldest-first input newest first
        page
    };
    let shown = Signal::derive(move || recordings.get().and_then(Result::ok).map(|l| page_of(l).into_iter().map(|r| r.id).collect()).unwrap_or_default());
    // Deleting the last clips of the last page: go back to the new last page.
    Effect::new(move || {
        let total = recordings.get().and_then(Result::ok).map_or(0, |l| l.len() as u32);
        let at = clip_page.get_untracked();
        if at > 0 && at * CLIPS_PAGE >= total {
            clip_page.set(total.saturating_sub(1) / CLIPS_PAGE);
        }
    });
    keep_only_shown(ticked, shown);
    let clips_view = move || state.with(|s| s.view == View::Clips);

    let selected = RwSignal::new(None::<Recording>);
    let open = Callback::new(move |r: Recording| selected.set(Some(r)));
    // DVR playback: camera, start time, and that camera's clips as they
    // were when clicked (the 30 s refresh must not restart playback).
    let dvr = RwSignal::new(None::<(String, chrono::DateTime<chrono::Utc>, Vec<Recording>)>);
    let playhead = RwSignal::new(None::<chrono::DateTime<chrono::Utc>>);
    let on_seek = Callback::new(move |(camera, at): (String, chrono::DateTime<chrono::Utc>)| {
        let mut clips: Vec<Recording> = recordings.get_untracked().and_then(Result::ok).unwrap_or_default().into_iter().filter(|r| r.camera_id == camera).collect();
        clips.sort_by_key(|r| r.start_time);
        dvr.set(Some((camera, at, clips)));
    });
    let zoom = Signal::derive(move || state.get().zoom);

    // + / − zoom the timeline.
    let keys = window_event_listener(ev::keydown, move |e| {
        let typing = e.target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlElement>(t).ok())
            .is_some_and(|el| matches!(el.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA"));
        let dir = match e.key().as_str() { "+" | "=" => 1, "-" | "_" => -1, _ => 0 };
        if typing || dir == 0 || state.get_untracked().view != View::Timeline {
            return;
        }
        let mut s = state.get_untracked();
        let i = ZOOMS.iter().position(|z| *z == s.zoom).unwrap_or(0) as i32;
        s.zoom = ZOOMS[(i + dir).clamp(0, ZOOMS.len() as i32 - 1) as usize];
        on_change.run(s);
    });
    on_cleanup(move || keys.remove());

    let subtitle = Signal::derive(move || {
        recordings.get().and_then(Result::ok).map(|l| {
            let bytes: u64 = l.iter().map(|r| r.file_size).sum();
            format!("{} clips · {}", l.len(), format::bytes(bytes))
        }).unwrap_or_default()
    });

    view! {
        <Page title="Recordings" subtitle>
            <Toolbar state cameras=camera_list on_change />
            {move || dvr.get().map(|(camera, at, clips)| {
                let camera_name = camera_list.get_untracked().into_iter().find(|c| c.id == camera).map(|c| c.name).unwrap_or(camera);
                view! { <DvrPlayer camera_name clips at playhead on_details=open on_close=Callback::new(move |_| dvr.set(None)) /> }
            })}
            {move || {
                let all = camera_list.get();
                if cameras.get().is_some() && all.is_empty() {
                    return view! { <NoCameras /> }.into_any();
                }
                match recordings.get() {
                    None => view! { <Skeleton lines=6 height="2.5rem" /> }.into_any(),
                    Some(Err(error)) => view! { <ErrorBox error /> }.into_any(),
                    Some(Ok(list)) => {
                        let s = state.get();
                        let shown: Vec<_> = all.into_iter().filter(|c| s.shows(&c.id)).collect();
                        match s.view {
                            View::Timeline => {
                                let (start, end) = s.day_range();
                                view! { <Timeline cameras=shown recordings=list events=events.get().and_then(Result::ok).map(|p| p.events).unwrap_or_default() scale=Scale::new(start, end) zoom on_seek playhead /> }.into_any()
                            }
                            View::Clips if list.is_empty() => view! {
                                <EmptyState icon=I::Film title="No recordings on this day" text="Pick another day or camera." />
                            }.into_any(),
                            View::Clips => {
                                let names: HashMap<String, String> = shown.into_iter().map(|c| (c.id, c.name)).collect();
                                let total = list.len() as u32;
                                let page = page_of(list);
                                let on_page = page.len() as u32;
                                view! {
                                    <ClipList recordings=page names selected=ticked on_open=open />
                                    <Pager page=clip_page per_page=CLIPS_PAGE total shown=on_page />
                                }.into_any()
                            }
                        }
                    }
                }
            }}
            <Show when=clips_view>
                <BulkBar selected=ticked shown matching=Signal::derive(move || {
                    let s = state.get();
                    let (from, to) = s.day_range();
                    recordings.get().and_then(Result::ok).map(|l| (RecordingQuery { camera_ids: s.cameras, from: Some(from), to: Some(to) }, l.len() as u32))
                }) />
            </Show>
            {move || selected.get().map(|recording| {
                let camera_name = camera_list.get_untracked().into_iter().find(|c| c.id == recording.camera_id).map(|c| c.name).unwrap_or_default();
                view! { <RecordingDrawer recording camera_name on_close=Callback::new(move |_| selected.set(None)) /> }
            })}
        </Page>
    }
}
