use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

use crate::api::{self, NotificationLevel, Topic, invalidate, use_query};
use crate::format;
use crate::ui::{Dot, I, Icon, Popover, Tone};

/// Bell with unread count and a dropdown feed.
#[component]
pub fn Notifications() -> impl IntoView {
    let open = RwSignal::new(false);
    let list = use_query(Topic::Notifications, Some(Duration::from_secs(30)), api::get_notifications);
    let items = move || list.get().and_then(Result::ok).unwrap_or_default();
    let unread = move || items().iter().filter(|n| !n.read).count();
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
                    n => Some(view! { <span class="badge-count">{n}</span> }),
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
                        let items = items();
                        if items.is_empty() {
                            return view! { <p class="notifications__empty">"You're all caught up."</p> }.into_any();
                        }
                        let navigate = navigate.clone();
                        view! {
                            <ul class="notifications__list">
                                {items
                                    .into_iter()
                                    .map(|n| {
                                        let tone = match n.level {
                                            NotificationLevel::Info => Tone::Accent,
                                            NotificationLevel::Success => Tone::Online,
                                            NotificationLevel::Warning => Tone::Warning,
                                            NotificationLevel::Error => Tone::Danger,
                                        };
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
                                                    <Dot tone dim=read />
                                                    <span class="notif__body">
                                                        <span class="notif__top">
                                                            <span class="notif__title truncate">{n.title}</span>
                                                            <span class="notif__time">{format::relative(n.time)}</span>
                                                        </span>
                                                        <span class="notif__msg truncate">{n.message}</span>
                                                    </span>
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
            </Popover>
        </div>
    }
}
