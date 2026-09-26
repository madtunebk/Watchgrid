use std::collections::BTreeSet;
use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

use super::bulk_bar::BulkBar;
use super::item::NotificationItem;
use crate::api::{self, Topic, use_query};
use crate::ui::{EmptyState, ErrorBox, I, Page, SelectCell, Skeleton, keep_only_shown};

const PAGE: u32 = 50;

#[component]
pub fn NotificationsPage() -> impl IntoView {
    let unread_only = RwSignal::new(false);
    // "Load more" grows the page; switching the filter starts over.
    let limit = RwSignal::new(PAGE);
    let selected = RwSignal::new(BTreeSet::<String>::new());
    Effect::new(move || {
        unread_only.track();
        limit.set(PAGE);
        selected.set(BTreeSet::new());
    });
    let page = use_query(Topic::Notifications, Some(Duration::from_secs(30)), move || api::get_notifications(unread_only.get(), limit.get(), 0));
    let loaded = move || page.get().and_then(Result::ok);
    let shown = Signal::derive(move || loaded().map(|p| p.items.into_iter().map(|n| n.id).collect()).unwrap_or_default());
    keep_only_shown(selected, shown);

    let subtitle = Signal::derive(move || loaded().map(|p| format!("{} unread", p.unread)).unwrap_or_default());
    let mark_all_read = move |_| {
        spawn_local(async move {
            if api::mark_notifications_read(None).await.is_ok() {
                api::invalidate(Topic::Notifications);
            }
        });
    };
    let open = {
        let navigate = use_navigate();
        Callback::new(move |(id, read, link): (String, bool, Option<String>)| {
            if !read {
                spawn_local(async move {
                    if api::mark_notifications_read(Some(vec![id])).await.is_ok() {
                        api::invalidate(Topic::Notifications);
                    }
                });
            }
            if let Some(link) = link {
                navigate(&link, Default::default());
            }
        })
    };

    let actions = ViewFn::from(move || {
        view! {
            <Show when=move || loaded().is_some_and(|p| p.unread > 0)>
                <button class="btn btn--secondary btn--sm" on:click=mark_all_read>"Mark all read"</button>
            </Show>
        }
    });

    view! {
        <Page title="Notifications" subtitle actions>
            <div class="filter-bar">
                <div class="filter-bar__row">
                    <div class="button-group" role="group" aria-label="Show">
                        <button class="button-group__btn" class:button-group__btn--active=move || !unread_only.get() on:click=move |_| unread_only.set(false)>"All"</button>
                        <button class="button-group__btn" class:button-group__btn--active=unread_only on:click=move |_| unread_only.set(true)>"Unread"</button>
                    </div>
                </div>
            </div>
            {move || match page.get() {
                None => view! { <Skeleton lines=8 height="3rem" /> }.into_any(),
                Some(Err(error)) => view! { <ErrorBox error /> }.into_any(),
                Some(Ok(p)) if p.items.is_empty() => {
                    if unread_only.get_untracked() {
                        view! { <EmptyState icon=I::Check title="Nothing unread" text="You're all caught up." /> }.into_any()
                    } else {
                        view! { <EmptyState icon=I::Bell title="No notifications" text="Camera problems, detections and storage warnings appear here." /> }.into_any()
                    }
                }
                Some(Ok(p)) => {
                    let shown_count = p.items.len() as u32;
                    view! {
                        <div class="evt-list notif-list">
                            {p.items.into_iter().map(|n| {
                                let is_selected = {
                                    let id = n.id.clone();
                                    Memo::new(move |_| selected.with(|s| s.contains(&id)))
                                };
                                let (read, args) = (n.read, (n.id.clone(), n.read, n.link.clone()));
                                view! {
                                    <div class="select-row" class:select-row--selected=is_selected>
                                        <SelectCell id=n.id.clone() selected label="Select notification" />
                                        <button class="notif" class:notif--read=read on:click=move |_| open.run(args.clone())>
                                            <NotificationItem notification=n />
                                        </button>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                        {(shown_count < p.total).then(|| view! {
                            <div class="load-more">
                                <span class="muted">{format!("Showing {shown_count} of {}", p.total)}</span>
                                <button class="btn btn--secondary btn--sm" on:click=move |_| limit.update(|l| *l += PAGE)>"Load more"</button>
                            </div>
                        })}
                    }.into_any()
                }
            }}
            <BulkBar selected shown />
        </Page>
    }
}
