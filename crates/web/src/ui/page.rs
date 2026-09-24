use leptos::prelude::*;

/// Standard page frame: compact title bar + content.
#[component]
pub fn Page(
    #[prop(into)] title: Signal<String>,
    #[prop(optional, into)] subtitle: Option<Signal<String>>,
    #[prop(optional, into)] actions: Option<ViewFn>,
    /// Remove content padding, e.g. for edge-to-edge video.
    #[prop(optional)]
    flush: bool,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="page">
            <div class="page__head">
                <div class="page__titles">
                    <h1 class="page__title truncate">{title}</h1>
                    {subtitle.map(|s| view! { <p class="page__subtitle truncate">{s}</p> })}
                </div>
                {actions.map(|a| view! { <div class="page__actions">{a.run()}</div> })}
            </div>
            <div class="page__body" class:page__body--flush=flush>{children()}</div>
        </div>
    }
}
