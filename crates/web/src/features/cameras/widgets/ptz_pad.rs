//! Pan / tilt controls for cameras that move (ONVIF PTZ). Hold an arrow to
//! turn: the move is repeated while held, and the camera stops by itself
//! about a second after the last one, so it never keeps turning.

use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, PtzMove, PtzPreset};
use crate::clock::use_interval;
use crate::ui::{I, Icon};

const SPEED: f32 = 0.5;
const REPEAT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    fn velocity(self) -> PtzMove {
        let (pan, tilt) = match self {
            Self::Up => (0.0, SPEED),
            Self::Down => (0.0, -SPEED),
            Self::Left => (-SPEED, 0.0),
            Self::Right => (SPEED, 0.0),
        };
        PtzMove { pan, tilt, zoom: 0.0 }
    }
}

/// Arrows (and, unless `compact`, presets). Renders nothing for cameras
/// that can't move.
#[component]
pub fn PtzPad(camera_id: String, #[prop(optional)] compact: bool) -> impl IntoView {
    let state = LocalResource::new({
        let id = camera_id.clone();
        move || api::get_ptz(id.clone())
    });
    let held = RwSignal::new(None::<Dir>);
    let error = RwSignal::new(None::<String>);
    let presets = RwSignal::new(Vec::<PtzPreset>::new());
    Effect::new(move || {
        if let Some(Ok(s)) = state.get() {
            presets.set(s.presets);
        }
    });

    let send = {
        let id = camera_id.clone();
        move |dir: Dir| {
            let id = id.clone();
            spawn_local(async move {
                if let Err(e) = api::ptz_move(id, dir.velocity()).await {
                    error.set(Some(e.to_string()));
                }
            });
        }
    };
    // Keep moving while held.
    use_interval(REPEAT, {
        let send = send.clone();
        move || {
            if let Some(dir) = held.get_untracked() {
                send(dir);
            }
        }
    });
    let press = {
        let send = send.clone();
        move |dir: Dir| {
            error.set(None);
            held.set(Some(dir));
            send(dir);
        }
    };
    let release = {
        let id = camera_id.clone();
        move || {
            if held.get_untracked().is_some() {
                held.set(None);
                let id = id.clone();
                spawn_local(async move {
                    let _ = api::ptz_stop(id).await;
                });
            }
        }
    };

    let arrow = move |dir: Dir, icon: I, label: &'static str, class: &'static str| {
        let (press, release, release2) = (press.clone(), release.clone(), release.clone());
        view! {
            <button class=format!("ptz__btn {class}") aria-label=label title=label
                class:ptz__btn--held=move || held.get() == Some(dir)
                on:pointerdown=move |ev| {
                    ev.prevent_default();
                    press(dir);
                }
                on:pointerup=move |_| release()
                on:pointerleave=move |_| release2()>
                <Icon icon=icon class="icon icon--sm" />
            </button>
        }
    };

    let available = Signal::derive(move || state.get().and_then(Result::ok).is_some_and(|s| s.available));

    view! {
        <Show when=move || available.get()>
            <div class="ptz" class:ptz--compact=compact on:dblclick=|ev| ev.stop_propagation()>
                <div class="ptz__pad">
                    {arrow(Dir::Up, I::ChevronUp, "Tilt up", "ptz__btn--up")}
                    {arrow(Dir::Left, I::ChevronLeft, "Pan left", "ptz__btn--left")}
                    {arrow(Dir::Right, I::ChevronRight, "Pan right", "ptz__btn--right")}
                    {arrow(Dir::Down, I::ChevronDown, "Tilt down", "ptz__btn--down")}
                </div>
                {(!compact).then(|| view! { <Presets camera_id=camera_id.clone() presets error /> })}
                {move || error.get().map(|e| view! { <p class="ptz__error">{e}</p> })}
            </div>
        </Show>
    }
}

#[component]
fn Presets(camera_id: String, presets: RwSignal<Vec<PtzPreset>>, error: RwSignal<Option<String>>) -> impl IntoView {
    let chosen = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    Effect::new(move || {
        let list = presets.get();
        if !list.iter().any(|p| p.token == chosen.get_untracked()) {
            chosen.set(list.first().map(|p| p.token.clone()).unwrap_or_default());
        }
    });
    let run = move |job: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>>>>| {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            if let Err(e) = job.await {
                error.set(Some(e));
            }
            busy.set(false);
        });
    };
    let goto = {
        let id = camera_id.clone();
        move |_| {
            let (id, token) = (id.clone(), chosen.get_untracked());
            if !token.is_empty() {
                run(Box::pin(async move { api::ptz_goto(id, token).await.map_err(|e| e.to_string()) }));
            }
        }
    };
    let save = {
        let id = camera_id.clone();
        move |_| {
            let (id, label) = (id.clone(), name.get_untracked().trim().to_string());
            if label.is_empty() {
                error.set(Some("Name the position first".into()));
                return;
            }
            run(Box::pin(async move {
                let preset = api::ptz_save_preset(id, label).await.map_err(|e| e.to_string())?;
                chosen.set(preset.token.clone());
                presets.update(|l| l.push(preset));
                name.set(String::new());
                Ok(())
            }));
        }
    };
    let remove = {
        let id = camera_id.clone();
        move |_| {
            let (id, token) = (id.clone(), chosen.get_untracked());
            if !token.is_empty() {
                run(Box::pin(async move {
                    api::ptz_remove_preset(id, token.clone()).await.map_err(|e| e.to_string())?;
                    presets.update(|l| l.retain(|p| p.token != token));
                    Ok(())
                }));
            }
        }
    };
    view! {
        <div class="ptz__presets">
            <div class="ptz__row">
                <select class="select select--sm" aria-label="Preset" disabled=move || presets.with(Vec::is_empty)
                    on:change=move |ev| chosen.set(event_target_value(&ev))>
                    {move || if presets.with(Vec::is_empty) {
                        view! { <option value="">"No saved positions"</option> }.into_any()
                    } else {
                        presets.get().into_iter().map(|p| {
                            let token = p.token.clone();
                            view! { <option value=p.token selected=move || chosen.get() == token>{p.name}</option> }
                        }).collect_view().into_any()
                    }}
                </select>
                <button class="btn btn--secondary btn--sm" disabled=move || busy.get() || chosen.get().is_empty() on:click=goto>
                    <Icon icon=I::MapPin class="icon icon--sm" />"Go"
                </button>
                <button class="icon-btn" aria-label="Delete this position" title="Delete this position"
                    disabled=move || busy.get() || chosen.get().is_empty() on:click=remove>
                    <Icon icon=I::Trash class="icon icon--sm" />
                </button>
            </div>
            <div class="ptz__row">
                <input class="input input--sm" placeholder="Name this position" maxlength="40" prop:value=name
                    on:input=move |ev| name.set(event_target_value(&ev)) />
                <button class="btn btn--secondary btn--sm" disabled=busy on:click=save>"Save"</button>
            </div>
        </div>
    }
}
