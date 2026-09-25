//! The tile grid, with drag & drop to rearrange cameras.

use std::collections::HashMap;

use leptos::ev::DragEvent;
use leptos::prelude::*;

use super::budget::prefers_substream;
use super::empty_slot::EmptySlot;
use super::layout::GridLayout;
use super::slots::Slots;
use super::tile::Tile;
use super::view_state::WallView;
use crate::api::Camera;

#[component]
pub fn Grid(
    layout: RwSignal<GridLayout>,
    /// Zero-based page of tiles to show.
    #[prop(into)]
    page: Signal<usize>,
    slots: Slots,
    #[prop(into)] cameras: Signal<HashMap<String, Camera>>,
    /// Opens a camera over the wall.
    view: WallView,
) -> impl IntoView {
    let dragging = RwSignal::new(None::<usize>);
    let over = RwSignal::new(None::<usize>);

    let available = Signal::derive(move || {
        let placed = slots.placed();
        let mut list: Vec<Camera> = cameras.get().into_values().filter(|c| !placed.contains(&c.id)).collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    });

    let cell = move |i: usize, substream: bool| {
        let id = slots.get(i);
        let camera = Signal::derive({
            let id = id.clone();
            move || id.as_ref().and_then(|id| cameras.get().get(id).cloned())
        });
        let on_drag_start = move |ev: DragEvent| {
            if let Some(dt) = ev.data_transfer() {
                let _ = dt.set_data("text/plain", &i.to_string());
                dt.set_effect_allowed("move");
            }
            dragging.set(Some(i));
        };
        let on_drop = move |ev: DragEvent| {
            ev.prevent_default();
            if let Some(from) = dragging.get_untracked() {
                slots.swap(from, i);
            }
            dragging.set(None);
            over.set(None);
        };
        let content = match id {
            Some(id) if camera.get_untracked().is_some() => view! {
                <Tile
                    camera
                    substream
                    on_focus=Callback::new({
                        let id = id.clone();
                        move |_| view.open(id.clone(), false)
                    })
                    on_fullscreen=Callback::new(move |_| view.open(id.clone(), true))
                    on_remove=Callback::new(move |_| slots.clear(i))
                />
            }
            .into_any(),
            _ => view! { <EmptySlot available on_pick=Callback::new(move |id| slots.assign(i, id)) /> }.into_any(),
        };
        view! {
            <div
                class="grid-cell"
                class:grid-cell--dragging=move || dragging.get() == Some(i)
                class:grid-cell--over=move || over.get() == Some(i) && dragging.get() != Some(i)
                draggable=move || if slots.get(i).is_some() { "true" } else { "false" }
                on:dragstart=on_drag_start
                on:dragend=move |_| { dragging.set(None); over.set(None); }
                on:dragover=move |ev: DragEvent| { ev.prevent_default(); over.set(Some(i)); }
                on:dragleave=move |_| if over.get_untracked() == Some(i) { over.set(None) }
                on:drop=on_drop
            >
                {content}
            </div>
        }
    };

    // Always mounted, also under an open camera: layout and streams survive.
    move || {
        let l = layout.get();
        let start = page.get() * l.cells();
        let substream = prefers_substream(l.columns());
        // Re-render cells when the arrangement changes.
        slots.track();
        view! {
            <div class="live-grid" style:--cols=l.columns().to_string()>
                {(start..start + l.cells()).map(|i| cell(i, substream)).collect_view()}
            </div>
        }
    }
}
