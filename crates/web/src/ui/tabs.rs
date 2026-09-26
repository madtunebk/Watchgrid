use leptos::prelude::*;

/// A tab that navigates to its own URL.
#[derive(Clone)]
pub struct Tab {
    pub key: &'static str,
    pub label: &'static str,
    pub href: String,
}

/// Horizontal tab bar. Uses plain links (the router handles them) so the
/// active tab is part of the URL and survives reloads.
#[component]
pub fn TabNav(tabs: Vec<Tab>, #[prop(into)] active: Signal<&'static str>) -> impl IntoView {
    view! {
        <nav class="tabs" role="tablist">
            {tabs.into_iter().map(|t| {
                let key = t.key;
                view! {
                    <a href=t.href role="tab" class="tabs__tab"
                        class:tabs__tab--active=move || active.get() == key
                        aria-selected=move || (active.get() == key).to_string()>
                        {t.label}
                    </a>
                }
            }).collect_view()}
        </nav>
    }
}
