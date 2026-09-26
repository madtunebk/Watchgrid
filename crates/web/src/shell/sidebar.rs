use leptos::prelude::*;
use leptos_router::components::A;

use super::nav::{ADMIN, NavItem, PRIMARY};
use crate::api::{self, Topic, use_query};
use crate::ui::{I, Icon};

#[component]
pub fn Sidebar(
    collapsed: RwSignal<bool>,
    #[prop(into)] mobile_open: Signal<bool>,
    on_close: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let server = use_query(Topic::Server, None, api::get_server_info);
    let server = move || server.get().and_then(Result::ok);

    view! {
        <div class="scrim" class:scrim--visible=mobile_open on:click=move |_| on_close() aria-hidden="true"></div>
        <aside
            class="sidebar"
            class:sidebar--open=mobile_open
            class:sidebar--collapsed=collapsed
            aria-label="Main navigation"
        >
            <div class="brand">
                <Logo />
                <div class="brand__text hide-collapsed">
                    <div class="brand__name truncate">"Watchgrid"</div>
                    // The server's own name, unless it is just "Watchgrid" again.
                    {move || server().map(|s| s.name).filter(|n| !n.trim().eq_ignore_ascii_case("watchgrid")).map(|name| view! {
                        <div class="brand__server truncate">{name}</div>
                    })}
                </div>
                <button class="icon-btn icon-btn--sm only-mobile" aria-label="Close menu" on:click=move |_| on_close()>
                    <Icon icon=I::X />
                </button>
            </div>

            <nav class="nav">
                <NavList items=PRIMARY collapsed />
                <div class="nav__section">"Administration"</div>
                <NavList items=ADMIN collapsed />
            </nav>

            <div class="sidebar__foot only-desktop">
                <button
                    class="icon-btn icon-btn--sm"
                    title=move || if collapsed.get() { "Expand sidebar" } else { "Collapse sidebar" }
                    aria-label=move || if collapsed.get() { "Expand sidebar" } else { "Collapse sidebar" }
                    on:click=move |_| collapsed.update(|c| *c = !*c)
                >
                    {move || {
                        let icon = if collapsed.get() { I::ChevronsRight } else { I::ChevronsLeft };
                        view! { <Icon icon /> }
                    }}
                </button>
                <span class="sidebar__version hide-collapsed">{move || server().map(|s| format!("v{}", s.version))}</span>
            </div>
        </aside>
    }
}

#[component]
fn NavList(items: &'static [NavItem], collapsed: RwSignal<bool>) -> impl IntoView {
    view! {
        <ul class="nav__list">
            {items
                .iter()
                .map(|item| {
                    view! {
                        <li>
                            <A
                                href=item.href
                                exact=item.href == "/"
                                attr:class="nav-link"
                                attr:title=move || collapsed.get().then_some(item.label)
                            >
                                <Icon icon=item.icon />
                                <span class="truncate hide-collapsed">{item.label}</span>
                            </A>
                        </li>
                    }
                })
                .collect_view()}
        </ul>
    }
}

#[component]
pub fn Logo() -> impl IntoView {
    view! {
        <svg class="brand__logo" viewBox="0 0 32 32" aria-hidden="true">
            <rect width="32" height="32" rx="6" fill="var(--surface-3)"></rect>
            <circle cx="16" cy="16" r="9" fill="none" stroke="var(--accent)" stroke-width="2.5"></circle>
            <circle cx="16" cy="16" r="3.5" fill="var(--recording)"></circle>
        </svg>
    }
}
