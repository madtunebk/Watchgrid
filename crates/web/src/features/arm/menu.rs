//! Header button: a lock, red with the count while cameras are armed; its
//! dropdown says what is armed and arms or disarms.

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::dialog::ArmDialog;
use crate::api::{self, Topic, use_query};
use crate::format;
use crate::ui::{I, Icon, Popover, Tone, use_toaster};

#[component]
pub fn ArmMenu() -> impl IntoView {
    let cameras = use_query(Topic::Cameras, None, api::get_cameras);
    let state = use_query(Topic::Cameras, None, api::get_arm);
    let open = RwSignal::new(false);
    let dialog = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let toaster = use_toaster();
    let current = move || state.get().and_then(Result::ok).unwrap_or_default();
    let has_cameras = move || cameras.get().and_then(Result::ok).is_some_and(|c| !c.is_empty());
    let all_armed = move || cameras.get().and_then(Result::ok).is_some_and(|c| c.iter().all(|c| current().cameras.contains(&c.id)));
    let name_of = move |id: &str| cameras.get().and_then(Result::ok).and_then(|l| l.into_iter().find(|c| c.id == id)).map_or_else(|| id.to_string(), |c| c.name);

    let disarm = move |_| {
        busy.set(true);
        spawn_local(async move {
            match api::disarm_cameras().await {
                Ok(_) => {
                    open.set(false);
                    toaster.show(Tone::Online, "Disarmed: every camera is back as it was");
                }
                Err(e) => toaster.show(Tone::Danger, format!("Couldn't disarm: {e}")),
            }
            busy.set(false);
        });
    };
    let choose = move |_| {
        open.set(false);
        dialog.set(true);
    };

    view! {
        <Show when=has_cameras>
            <div class="popover-anchor">
                <button
                    class="icon-btn"
                    class:icon-btn--active=open
                    class:icon-btn--armed=move || current().armed
                    aria-label=move || match current().cameras.len() {
                        0 => "Disarmed".to_string(),
                        n => format!("Armed ({n} cameras)"),
                    }
                    title=move || if current().armed { "Armed" } else { "Disarmed" }
                    on:click=move |_| open.update(|o| *o = !*o)
                >
                    {move || if current().armed { view! { <Icon icon=I::Lock /> } } else { view! { <Icon icon=I::LockOpen /> } }}
                    {move || match current().cameras.len() {
                        0 => None,
                        n => Some(view! { <span class="badge-count">{n}</span> }),
                    }}
                </button>
                <Popover open class="arm-menu">
                    {move || {
                        let s = current();
                        if s.armed {
                            let since = s.since.map(format::time_of_day).unwrap_or_default();
                            let names: Vec<String> = s.cameras.iter().map(|id| name_of(id)).collect();
                            view! {
                                <div class="popover__head">
                                    <span class="popover__title arm-menu__armed">
                                        <Icon icon=I::Lock class="icon icon--sm" />
                                        {format!("Armed since {since}")}
                                    </span>
                                </div>
                                <p class="arm-menu__text">"These cameras detect motion themselves and record on it:"</p>
                                <ul class="arm-menu__list">
                                    {names.into_iter().map(|n| view! { <li>{n}</li> }).collect_view()}
                                </ul>
                                <div class="arm-menu__actions">
                                    <Show when=move || !all_armed()>
                                        <button class="btn btn--secondary btn--sm" disabled=busy on:click=choose>"Arm more…"</button>
                                    </Show>
                                    <button class="btn btn--primary btn--sm" disabled=busy on:click=disarm>
                                        <Icon icon=I::LockOpen class="icon icon--sm" />"Disarm"
                                    </button>
                                </div>
                            }
                            .into_any()
                        } else {
                            view! {
                                <div class="popover__head">
                                    <span class="popover__title">"Disarmed"</span>
                                </div>
                                <p class="arm-menu__text">"Arm the cameras when nobody is home: they detect motion themselves and record on it. Disarm puts each one back as it was."</p>
                                <div class="arm-menu__actions">
                                    <button class="btn btn--primary btn--sm" on:click=choose>
                                        <Icon icon=I::Lock class="icon icon--sm" />"Arm cameras…"
                                    </button>
                                </div>
                            }
                            .into_any()
                        }
                    }}
                </Popover>
            </div>
            <ArmDialog open=dialog cameras armed=Signal::derive(move || current().cameras) />
        </Show>
    }
}
