use leptos::ev;
use leptos::prelude::*;

/// Modal confirmation for destructive or important actions.
#[component]
pub fn ConfirmDialog(
    open: RwSignal<bool>,
    title: &'static str,
    #[prop(into)] message: Signal<String>,
    confirm_label: &'static str,
    #[prop(optional)] danger: bool,
    #[prop(optional, into)] busy: Signal<bool>,
    #[prop(optional, into)] error: Signal<Option<String>>,
    on_confirm: Callback<()>,
) -> impl IntoView {
    let esc = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" && !busy.get_untracked() {
            open.set(false);
        }
    });
    on_cleanup(move || esc.remove());

    view! {
        <Show when=move || open.get()>
            <div class="modal-backdrop" on:click=move |_| if !busy.get() { open.set(false) }></div>
            <div class="modal" role="alertdialog" aria-modal="true" aria-label=title>
                <h2 class="modal__title">{title}</h2>
                <p class="modal__text">{message}</p>
                {move || error.get().map(|e| view! { <p class="modal__error">{e}</p> })}
                <div class="modal__actions">
                    <button class="btn btn--secondary" disabled=busy on:click=move |_| open.set(false)>"Cancel"</button>
                    <button class="btn" class:btn--danger-solid=danger class:btn--primary=!danger disabled=busy
                        on:click=move |_| on_confirm.run(())>
                        {move || if busy.get() { "Working…" } else { confirm_label }}
                    </button>
                </div>
            </div>
        </Show>
    }
}
