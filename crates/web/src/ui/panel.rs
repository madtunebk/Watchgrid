use leptos::prelude::*;
use leptos_router::components::A;

use super::icons::{I, Icon};

/// Titled content block used for dashboard-style summaries.
#[component]
pub fn Panel(
    title: &'static str,
    /// Optional "see more" link in the header.
    #[prop(optional)]
    link: Option<(&'static str, &'static str)>,
    children: Children,
) -> impl IntoView {
    view! {
        <section class="panel">
            <header class="panel__head">
                <h2 class="panel__title">{title}</h2>
                {link.map(|(label, href)| view! {
                    <A href=href attr:class="panel__link">
                        {label}
                        <Icon icon=I::ArrowRight class="icon icon--sm" />
                    </A>
                })}
            </header>
            <div class="panel__body">{children()}</div>
        </section>
    }
}
