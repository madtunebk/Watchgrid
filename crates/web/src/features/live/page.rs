use std::collections::HashMap;
use std::time::Duration;

use leptos::ev;
use leptos::html::Div;
use leptos::prelude::*;

use super::budget::page_load;
use super::footer::WallFooter;
use super::grid::Grid;
use super::layout::GridLayout;
use super::slots::Slots;
use crate::api::{self, Topic, use_query};
use crate::clock::use_interval;
use crate::features::cameras::NoCameras;
use crate::prefs;
use crate::ui::form::Segmented;
use crate::ui::{I, Icon, Page, Skeleton, fullscreen};

const LAYOUT_KEY: &str = "ui.live.layout";
const CYCLE_EVERY: Duration = Duration::from_secs(15);

#[component]
pub fn LiveViewPage() -> impl IntoView {
    // Poll so tiles show current motion / recording / connection state.
    let cameras = use_query(Topic::Cameras, Some(Duration::from_secs(5)), api::get_cameras);
    let list = Memo::new(move |_| cameras.get().and_then(Result::ok));
    let by_id = Signal::derive(move || {
        list.get().unwrap_or_default().into_iter().map(|c| (c.id.clone(), c)).collect::<HashMap<_, _>>()
    });

    let (slots, arranged_before) = Slots::load();
    let saved_layout = prefs::get(LAYOUT_KEY).and_then(|k| GridLayout::from_key(&k));
    let layout = RwSignal::new(saved_layout.unwrap_or(GridLayout::Two));
    Effect::new(move || prefs::set(LAYOUT_KEY, layout.get().key()));
    let focused = RwSignal::new(None::<String>);
    let page = RwSignal::new(0usize);
    let cycle = RwSignal::new(false);

    // Enough pages for every placed camera, plus room for unplaced ones.
    let pages = Memo::new(move |_| {
        let cells = layout.get().cells();
        slots.track();
        let needed = slots.extent().max(list.get().map_or(0, |l| l.len()));
        needed.div_ceil(cells).max(1)
    });
    Effect::new(move || {
        let last = pages.get() - 1;
        if page.get_untracked() > last {
            page.set(last);
        }
    });
    use_interval(CYCLE_EVERY, move || {
        if cycle.get_untracked() && focused.get_untracked().is_none() {
            page.update(|p| *p = (*p + 1) % pages.get_untracked());
        }
    });

    let load = Signal::derive(move || {
        let l = layout.get();
        let map = by_id.get();
        let on_page: Vec<_> = slots.range(page.get() * l.cells(), l.cells()).iter().filter_map(|id| map.get(id).cloned()).collect();
        page_load(&on_page, l.columns())
    });

    // First visit (or nothing left to show): place cameras in order and pick
    // a layout that fits them. Otherwise only drop cameras that were deleted.
    let first_run = StoredValue::new(!arranged_before);
    Effect::new(move || {
        let Some(list) = list.get() else { return };
        slots.forget_missing(&list);
        if first_run.get_value() || slots.placed().is_empty() {
            first_run.set_value(false);
            slots.arrange(&list);
            if saved_layout.is_none() {
                layout.set(GridLayout::fitting(list.len()));
            }
        }
    });

    let keys = window_event_listener(ev::keydown, move |e| {
        let typing = e.target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlElement>(t).ok())
            .is_some_and(|el| matches!(el.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA"));
        if typing || e.ctrl_key() || e.meta_key() || e.alt_key() {
            return;
        }
        let n = pages.get_untracked();
        match e.key().as_str() {
            "Escape" => focused.set(None),
            "ArrowRight" | "PageDown" => page.update(|p| *p = (*p + 1) % n),
            "ArrowLeft" | "PageUp" => page.update(|p| *p = (*p + n - 1) % n),
            k => {
                if let Some(l) = GridLayout::from_key(k) {
                    focused.set(None);
                    layout.set(l);
                }
            }
        }
    });
    on_cleanup(move || keys.remove());

    let wall = NodeRef::<Div>::new();
    let subtitle = Signal::derive(move || {
        list.get().map(|l| {
            let n = l.iter().filter(|c| c.streaming()).count();
            format!("{n} of {} cameras live", l.len())
        }).unwrap_or_default()
    });

    view! {
        <Page
            title="Live View"
            subtitle
            flush=true
            actions=move || view! {
                <Segmented value=layout label="Grid layout"
                    options=GridLayout::ALL.into_iter().map(|l| (l, view! {
                        <Icon icon=l.icon() class="icon icon--sm" /><span>{l.label()}</span>
                    }.into_any())).collect() />
                <button class="btn btn--secondary btn--sm" title="Place all cameras in order"
                    on:click=move |_| if let Some(l) = list.get_untracked() { focused.set(None); page.set(0); slots.arrange(&l); }>
                    <Icon icon=I::RotateCcw class="icon icon--sm" />"Auto-arrange"
                </button>
                <button class="btn btn--secondary btn--sm" title="Show the whole grid fullscreen"
                    on:click=move |_| if let Some(el) = wall.get() { fullscreen::toggle(&el) }>
                    <Icon icon=I::Maximize class="icon icon--sm" />"Fullscreen"
                </button>
            }
        >
            <div class="live-wall" node_ref=wall>
                {move || match list.get() {
                    None => view! { <div class="live-wall__loading"><Skeleton lines=1 height="60vh" /></div> }.into_any(),
                    Some(l) if l.is_empty() => view! { <div class="live-wall__empty"><NoCameras /></div> }.into_any(),
                    Some(_) => view! { <Grid layout page slots cameras=by_id focused /> }.into_any(),
                }}
                <WallFooter page pages cycle load />
            </div>
        </Page>
    }
}
