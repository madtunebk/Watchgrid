use leptos::prelude::*;

use super::icons::{I, Icon};

/// Intentional "nothing here yet" state with an optional call to action.
#[component]
pub fn EmptyState(
    icon: I,
    title: &'static str,
    #[prop(optional)] text: &'static str,
    /// Use the compact variant inside panels.
    #[prop(optional)]
    compact: bool,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="empty" class:empty--compact=compact>
            <div class="empty__icon"><Icon icon class="icon icon--xl" /></div>
            <p class="empty__title">{title}</p>
            {(!text.is_empty()).then(|| view! { <p class="empty__text">{text}</p> })}
            {children.map(|c| view! { <div class="empty__actions">{c()}</div> })}
        </div>
    }
}
