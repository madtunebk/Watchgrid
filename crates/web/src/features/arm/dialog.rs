//! The camera checklist for arming: every camera not yet armed, all ticked.

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, Camera, Id};
use crate::ui::{Tone, use_toaster};

/// The camera checklist: every camera not yet armed, all ticked.
#[component]
pub fn ArmDialog(open: RwSignal<bool>, cameras: LocalResource<api::ApiResult<Vec<Camera>>>, armed: Signal<Vec<Id>>) -> impl IntoView {
    let chosen = RwSignal::new(Vec::<Id>::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let toaster = use_toaster();
    let candidates = move || cameras.get().and_then(Result::ok).unwrap_or_default().into_iter().filter(|c| !armed.get().contains(&c.id)).collect::<Vec<_>>();

    // Opening ticks every candidate.
    Effect::new(move || {
        if open.get() {
            chosen.set(candidates().into_iter().map(|c| c.id).collect());
            error.set(None);
        }
    });
    let esc = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" && !busy.get_untracked() {
            open.set(false);
        }
    });
    on_cleanup(move || esc.remove());

    let toggle = move |id: Id| chosen.update(|c| if let Some(i) = c.iter().position(|x| *x == id) { c.remove(i); } else { c.push(id); });
    let arm = move |_| {
        let ids = chosen.get_untracked();
        busy.set(true);
        spawn_local(async move {
            match api::arm_cameras(ids).await {
                Ok(s) => {
                    open.set(false);
                    toaster.show(Tone::Recording, format!("Armed: {} watching for motion", match s.cameras.len() { 1 => "1 camera".to_string(), n => format!("{n} cameras") }));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <Show when=move || open.get()>
            <div class="modal-backdrop" on:click=move |_| if !busy.get() { open.set(false) }></div>
            <div class="modal" role="dialog" aria-modal="true" aria-label="Arm cameras">
                <h2 class="modal__title">"Arm cameras"</h2>
                <p class="modal__text">"Armed cameras detect motion with Watchgrid's own detection and record on it. Disarm puts each camera back as it was."</p>
                <div class="arm-list">
                    {move || {
                        let list = candidates();
                        if list.is_empty() {
                            return view! { <p class="note">"Every camera is already armed."</p> }.into_any();
                        }
                        list.into_iter().map(|c| {
                            let id = c.id.clone();
                            let checked = { let id = id.clone(); move || chosen.get().contains(&id) };
                            view! {
                                <label class="check-row">
                                    <input type="checkbox" prop:checked=checked on:change=move |_| toggle(id.clone()) />
                                    {c.name}
                                </label>
                            }
                        }).collect_view().into_any()
                    }}
                </div>
                {move || error.get().map(|e| view! { <p class="modal__error">{e}</p> })}
                <div class="modal__actions">
                    <button class="btn btn--secondary" disabled=busy on:click=move |_| open.set(false)>"Cancel"</button>
                    <button class="btn btn--primary" disabled=move || busy.get() || chosen.with(Vec::is_empty) on:click=arm>
                        {move || match (busy.get(), chosen.with(Vec::len)) {
                            (true, _) => "Arming…".to_string(),
                            (_, 1) => "Arm 1 camera".to_string(),
                            (_, n) => format!("Arm {n} cameras"),
                        }}
                    </button>
                </div>
            </div>
        </Show>
    }
}
