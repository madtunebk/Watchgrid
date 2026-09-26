use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::api::{self, Topic, invalidate, use_query};
use crate::features::notifications::NotificationItem;
use crate::ui::{I, Icon, Popover};

/// Latest notifications in the dropdown; the rest are on the Notifications page.
const LATEST: u32 = 15;

/// Bell with the unread count and a dropdown of the latest notifications.
#[component]
pub fn Notifications() -> impl IntoView {
    let open = RwSignal::new(false);
    let page = use_query(Topic::Notifications, Some(Duration::from_secs(30)), || api::get_notifications(false, LATEST, 0));
    let unread = move || page.get().and_then(Result::ok).map_or(0, |p| p.unread);
    let navigate = use_navigate();

    let mark_read = move |ids: Option<Vec<String>>| {
        spawn_local(async move {
            if api::mark_notifications_read(ids).await.is_ok() {
                invalidate(Topic::Notifications);
            }
        });
    };

    view! {
        <div class="popover-anchor">
            <button
                class="icon-btn"
                class:icon-btn--active=open
                aria-label=move || match unread() {
                    0 => "Notifications".to_string(),
                    n => format!("Notifications ({n} unread)"),
                }
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <Icon icon=I::Bell />
                {move || match unread() {
                    0 => None,
                    n => Some(view! { <span class="badge-count">{if n > 99 { "99+".to_string() } else { n.to_string() }}</span> }),
                }}
            </button>
            <Popover open class="notifications">
                <div class="popover__head">
                    <span class="popover__title">"Notifications"</span>
                    <Show when=move || { unread() > 0 }>
                        <button class="link-btn" on:click=move |_| mark_read(None)>"Mark all read"</button>
                    </Show>
                </div>
                {
                    let navigate = navigate.clone();
                    move || {
                        let items = match page.get() {
                            None => return view! { <p class="notifications__empty">"Loading…"</p> }.into_any(),
                            Some(Err(_)) => return view! { <p class="notifications__empty">"Couldn't load notifications."</p> }.into_any(),
                            Some(Ok(p)) => p.items,
                        };
                        if items.is_empty() {
                            return view! { <p class="notifications__empty">"You're all caught up."</p> }.into_any();
                        }
                        let navigate = navigate.clone();
                        view! {
                            <ul class="notifications__list">
                                {items
                                    .into_iter()
                                    .map(|n| {
                                        let navigate = navigate.clone();
                                        let (id, read, link) = (n.id.clone(), n.read, n.link.clone());
                                        view! {
                                            <li>
                                                <button
                                                    class="notif"
                                                    class:notif--read=read
                                                    on:click=move |_| {
                                                        if !read {
                                                            mark_read(Some(vec![id.clone()]));
                                                        }
                                                        open.set(false);
                                                        if let Some(link) = &link {
                                                            navigate(link, Default::default());
                                                        }
                                                    }
                                                >
                                                    <NotificationItem notification=n />
                                                </button>
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                        }
                        .into_any()
                    }
                }
                <div class="popover__foot">
                    <A href="/notifications" attr:class="link-btn" on:click=move |_| open.set(false)>"All notifications"</A>
                </div>
            </Popover>
        </div>
    }
}
