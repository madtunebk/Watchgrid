//! Motion zone editor over the camera's live preview: drag on the picture
//! to draw a zone, drag a zone to move it, edit or remove it in the list.
//! Coordinates are fractions of the picture (0-1), like the server stores.

use leptos::ev::PointerEvent;
use leptos::html::Div;
use leptos::prelude::*;

use crate::api::{Camera, MotionZone};
use crate::features::cameras::widgets::CameraPreview;
use crate::ui::{I, Icon};

/// As many as the server accepts.
pub const MAX_ZONES: usize = 16;
/// Smaller drags are clicks, not zones.
const MIN_SIZE: f32 = 0.02;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    /// Drawing a new zone from this corner.
    Draw { from: (f32, f32) },
    /// Moving zone `index`, grabbed at this offset from its corner.
    Move { index: usize, grab: (f32, f32) },
}

/// Rectangle between two corners, as (x, y, w, h).
fn span(a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32, f32) {
    (a.0.min(b.0), a.1.min(b.1), (a.0 - b.0).abs(), (a.1 - b.1).abs())
}

fn next_id(zones: &[MotionZone]) -> (String, String) {
    let n = (1..).find(|n| !zones.iter().any(|z| z.id == format!("zone-{n}"))).unwrap_or(1);
    (format!("zone-{n}"), format!("Zone {n}"))
}

#[component]
pub fn ZoneEditor(camera: Signal<Camera>, zones: RwSignal<Vec<MotionZone>>, #[prop(into)] disabled: Signal<bool>) -> impl IntoView {
    let stage = NodeRef::<Div>::new();
    let drag = RwSignal::new(None::<Drag>);
    let draft = RwSignal::new(None::<(f32, f32, f32, f32)>);
    let selected = RwSignal::new(None::<String>);

    // Pointer position as a fraction of the picture, clamped to it.
    let point = move |ev: &PointerEvent| -> Option<(f32, f32)> {
        let r = stage.get_untracked()?.get_bounding_client_rect();
        if r.width() <= 0.0 || r.height() <= 0.0 {
            return None;
        }
        let x = ((f64::from(ev.client_x()) - r.left()) / r.width()).clamp(0.0, 1.0);
        let y = ((f64::from(ev.client_y()) - r.top()) / r.height()).clamp(0.0, 1.0);
        Some((x as f32, y as f32))
    };
    // Keep receiving moves when the pointer leaves the picture mid-drag.
    let capture = move |ev: &PointerEvent| {
        if let Some(el) = stage.get_untracked() {
            let _ = el.set_pointer_capture(ev.pointer_id());
        }
    };

    let start_draw = move |ev: PointerEvent| {
        if disabled.get_untracked() || ev.button() != 0 || zones.with_untracked(Vec::len) >= MAX_ZONES {
            return;
        }
        if let Some(p) = point(&ev) {
            ev.prevent_default();
            capture(&ev);
            selected.set(None);
            drag.set(Some(Drag::Draw { from: p }));
            draft.set(Some((p.0, p.1, 0.0, 0.0)));
        }
    };
    let start_move = move |ev: PointerEvent, index: usize| {
        ev.stop_propagation();
        if disabled.get_untracked() || ev.button() != 0 {
            return;
        }
        let (Some(p), Some(z)) = (point(&ev), zones.with_untracked(|z| z.get(index).cloned())) else { return };
        ev.prevent_default();
        capture(&ev);
        selected.set(Some(z.id.clone()));
        drag.set(Some(Drag::Move { index, grab: (p.0 - z.x, p.1 - z.y) }));
    };
    let on_move = move |ev: PointerEvent| {
        let (Some(d), Some(p)) = (drag.get_untracked(), point(&ev)) else { return };
        match d {
            Drag::Draw { from } => draft.set(Some(span(from, p))),
            Drag::Move { index, grab } => zones.update(|list| {
                if let Some(z) = list.get_mut(index) {
                    z.x = (p.0 - grab.0).clamp(0.0, 1.0 - z.w);
                    z.y = (p.1 - grab.1).clamp(0.0, 1.0 - z.h);
                }
            }),
        }
    };
    let on_up = move |_: PointerEvent| {
        if let (Some(Drag::Draw { .. }), Some((x, y, w, h))) = (drag.get_untracked(), draft.get_untracked())
            && w >= MIN_SIZE
            && h >= MIN_SIZE
        {
            zones.update(|list| {
                let (id, name) = next_id(list);
                selected.set(Some(id.clone()));
                list.push(MotionZone { id, name, x, y, w, h, exclude: false });
            });
        }
        drag.set(None);
        draft.set(None);
    };

    let remove = move |id: String| {
        zones.update(|list| list.retain(|z| z.id != id));
        if selected.get_untracked().as_ref() == Some(&id) {
            selected.set(None);
        }
    };
    let edit = move |id: String, f: Box<dyn FnOnce(&mut MotionZone)>| {
        zones.update(|list| {
            if let Some(z) = list.iter_mut().find(|z| z.id == id) {
                f(z);
            }
        });
    };

    view! {
        <div class="zone-editor" class:zone-editor--disabled=disabled>
            <div class="zone-stage zone-stage--edit" node_ref=stage
                on:pointerdown=start_draw on:pointermove=on_move on:pointerup=on_up on:pointercancel=on_up>
                {move || view! { <CameraPreview camera=camera.get() substream=true /> }}
                <svg class="zone-stage__svg zone-stage__svg--edit" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
                    {move || zones.get().into_iter().enumerate().map(|(i, z)| {
                        let active = selected.get().as_deref() == Some(z.id.as_str());
                        view! {
                            <rect x=z.x * 100.0 y=z.y * 100.0 width=z.w * 100.0 height=z.h * 100.0
                                class=if z.exclude { "zone zone--exclude" } else { "zone" } class:zone--selected=active
                                on:pointerdown=move |ev| start_move(ev, i) />
                        }
                    }).collect_view()}
                    {move || draft.get().map(|(x, y, w, h)| view! {
                        <rect class="zone zone--draft" x=x * 100.0 y=y * 100.0 width=w * 100.0 height=h * 100.0 />
                    })}
                </svg>
                <div class="zone-stage__labels">
                    {move || zones.get().into_iter().map(|z| view! {
                        <span class="zone-label" class:zone-label--exclude=z.exclude
                            style:left=format!("{}%", z.x * 100.0) style:top=format!("{}%", (z.y + z.h) * 100.0)>
                            {z.name}
                        </span>
                    }).collect_view()}
                </div>
                {move || zones.with(Vec::is_empty).then(|| view! {
                    <div class="zone-stage__empty">"Whole picture — drag on it to draw a zone"</div>
                })}
            </div>

            <ul class="zone-list">
                {move || zones.get().into_iter().map(|z| {
                    let (id_name, id_kind, id_del, id_sel) = (z.id.clone(), z.id.clone(), z.id.clone(), z.id.clone());
                    let active = selected.get().as_deref() == Some(z.id.as_str());
                    view! {
                        <li class="zone-row" class:zone-row--selected=active on:click=move |_| selected.set(Some(id_sel.clone()))>
                            <span class="zone-row__swatch" class:zone-row__swatch--exclude=z.exclude></span>
                            <input class="input zone-row__name" maxlength="40" prop:value=z.name.clone() disabled=disabled
                                aria-label="Zone name"
                                on:change=move |ev| {
                                    let name = event_target_value(&ev).trim().to_string();
                                    if !name.is_empty() {
                                        edit(id_name.clone(), Box::new(move |z| z.name = name));
                                    }
                                } />
                            <select class="select zone-row__kind" disabled=disabled aria-label="Zone type"
                                on:change=move |ev| {
                                    let exclude = event_target_value(&ev) == "exclude";
                                    edit(id_kind.clone(), Box::new(move |z| z.exclude = exclude));
                                }>
                                <option value="include" selected=!z.exclude>"Detect here"</option>
                                <option value="exclude" selected=z.exclude>"Ignore here"</option>
                            </select>
                            <button class="icon-btn" aria-label="Remove zone" title="Remove zone" disabled=disabled
                                on:click=move |ev| {
                                    ev.stop_propagation();
                                    remove(id_del.clone());
                                }>
                                <Icon icon=I::Trash class="icon icon--sm" />
                            </button>
                        </li>
                    }
                }).collect_view()}
            </ul>
            {move || (zones.with(Vec::len) >= MAX_ZONES).then(|| view! { <p class="note">{format!("At most {MAX_ZONES} zones.")}</p> })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_any_drag_direction_and_ids_fill_gaps() {
        assert_eq!(span((0.5, 0.5), (0.25, 0.75)), (0.25, 0.5, 0.25, 0.25));
        let z = |id: &str| MotionZone { id: id.into(), name: String::new(), x: 0.0, y: 0.0, w: 0.1, h: 0.1, exclude: false };
        assert_eq!(next_id(&[z("zone-1"), z("zone-3")]).0, "zone-2");
        assert_eq!(next_id(&[]), ("zone-1".to_string(), "Zone 1".to_string()));
    }
}
