//! Short messages in a corner of the screen that go away by themselves.

use std::time::Duration;

use leptos::prelude::*;

use super::icons::{I, Icon};
use super::status::Tone;

/// How long a message stays on screen.
const SHOWN_FOR: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct Toast {
    id: u32,
    tone: Tone,
    message: String,
}

/// Handle for showing messages; take it with [`use_toaster`] while the
/// component is built, then call it from anywhere (async tasks included).
#[derive(Clone, Copy)]
pub struct Toaster {
    list: RwSignal<Vec<Toast>>,
    next: StoredValue<u32>,
}

impl Toaster {
    pub fn show(self, tone: Tone, message: impl Into<String>) {
        let id = self.next.get_value();
        self.next.set_value(id.wrapping_add(1));
        self.list.update(|l| l.push(Toast { id, tone, message: message.into() }));
        set_timeout(move || self.dismiss(id), SHOWN_FOR);
    }

    fn dismiss(self, id: u32) {
        self.list.try_update(|l| l.retain(|t| t.id != id));
    }
}

pub fn provide_toaster() {
    provide_context(Toaster { list: RwSignal::new(Vec::new()), next: StoredValue::new(0) });
}

pub fn use_toaster() -> Toaster {
    expect_context::<Toaster>()
}

/// Where the messages appear; render once, at the root.
#[component]
pub fn ToastHost() -> impl IntoView {
    let toaster = use_toaster();
    view! {
        <div class="toasts" role="status" aria-live="polite">
            <For each=move || toaster.list.get() key=|t| t.id let:t>
                <div class=format!("toast toast--{}", t.tone.class())>
                    <Icon icon=if matches!(t.tone, Tone::Danger | Tone::Warning) { I::TriangleAlert } else { I::Check } class="icon icon--sm" />
                    <span class="toast__text">{t.message}</span>
                    <button class="toast__close" aria-label="Dismiss" on:click=move |_| toaster.dismiss(t.id)>
                        <Icon icon=I::X class="icon icon--sm" />
                    </button>
                </div>
            </For>
        </div>
    }
}
