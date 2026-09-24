//! Events page: the main way to find recordings, grouped by day.

mod day_groups;
mod filter_bar;
mod filters;
mod list_row;

use std::collections::HashMap;
use std::time::Duration;

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_query_map};

use crate::api::{self, Topic, use_query};
use crate::prefs;
use crate::ui::{EmptyState, ErrorBox, I, Page, Skeleton};
use filter_bar::FilterBar;
use filters::Filters;
use list_row::EventListRow;

const PAGE: u32 = 100;
/// Where the details page's "Back" returns to (keeps the filters).
pub const BACK_KEY: &str = "ui.events.back";

#[component]
pub fn EventsPage() -> impl IntoView {
    let query = use_query_map();
    let filters = Memo::new(move |_| query.with(Filters::from_query));
    let navigate = use_navigate();
    let on_change = Callback::new(move |f: Filters| navigate(&f.to_url(), leptos_router::NavigateOptions { replace: true, ..Default::default() }));

    // "Load more" grows the page; any filter change starts over.
    let limit = RwSignal::new(PAGE);
    Effect::new(move || {
        let url = filters.with(Filters::to_url);
        prefs::set(BACK_KEY, &url);
        limit.set(PAGE);
    });

    let cameras = use_query(Topic::Cameras, None, api::get_cameras);
    let camera_list = Signal::derive(move || cameras.get().and_then(Result::ok).unwrap_or_default());
    let names = Memo::new(move |_| camera_list.get().into_iter().map(|c| (c.id, c.name)).collect::<HashMap<_, _>>());
    let events = use_query(Topic::Events, Some(Duration::from_secs(30)), move || api::get_events(filters.get().to_query(limit.get())));

    let subtitle = Signal::derive(move || {
        events.get().and_then(Result::ok).map(|p| format!("{} event{}", p.total, if p.total == 1 { "" } else { "s" })).unwrap_or_default()
    });

    view! {
        <Page title="Events" subtitle>
            <FilterBar filters cameras=camera_list on_change />
            {move || match events.get() {
                None => view! { <Skeleton lines=8 height="3rem" /> }.into_any(),
                Some(Err(error)) => view! { <ErrorBox error /> }.into_any(),
                Some(Ok(page)) if page.events.is_empty() => {
                    if camera_list.get().is_empty() {
                        view! { <EmptyState icon=I::Activity title="No events yet"
                            text="Events appear here once cameras are added and detect motion or you record manually." /> }.into_any()
                    } else {
                        view! { <EmptyState icon=I::Search title="No events match"
                            text="Try another date range or fewer filters." /> }.into_any()
                    }
                }
                Some(Ok(page)) => {
                    let shown = page.events.len() as u32;
                    let names = names.get();
                    view! {
                        {day_groups::by_day(page.events).into_iter().map(|(day, list)| {
                            let count = list.len();
                            view! {
                                <section class="day-group">
                                    <h2 class="day-group__title">
                                        {day_groups::heading(day)}
                                        <span class="day-group__count">{count}</span>
                                    </h2>
                                    <div class="evt-list">
                                        {list.into_iter().map(|event| {
                                            let camera_name = names.get(&event.camera_id).cloned().unwrap_or_else(|| event.camera_id.clone());
                                            view! { <EventListRow event camera_name /> }
                                        }).collect_view()}
                                    </div>
                                </section>
                            }
                        }).collect_view()}
                        {(shown < page.total).then(|| view! {
                            <div class="load-more">
                                <span class="muted">{format!("Showing {shown} of {}", page.total)}</span>
                                <button class="btn btn--secondary btn--sm" on:click=move |_| limit.update(|l| *l += PAGE)>"Load more"</button>
                            </div>
                        })}
                    }.into_any()
                }
            }}
        </Page>
    }
}
