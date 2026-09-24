use leptos::prelude::*;

use super::filter::StatusFilter;
use super::ViewMode;
use crate::ui::form::Segmented;
use crate::ui::{I, Icon};

/// Search, status filter and grid/list switch.
#[component]
pub fn Toolbar(search: RwSignal<String>, status: RwSignal<StatusFilter>, view_mode: RwSignal<ViewMode>) -> impl IntoView {
    let filters = [
        (StatusFilter::All, "All"),
        (StatusFilter::Online, "Online"),
        (StatusFilter::Offline, "Offline"),
        (StatusFilter::Recording, "Recording"),
        (StatusFilter::Disabled, "Disabled"),
    ];

    view! {
        <div class="toolbar">
            <label class="search">
                <Icon icon=I::Search class="icon icon--sm" />
                <input class="search__input" type="search" placeholder="Search name, host or location" bind:value=search />
            </label>
            <Segmented value=status label="Filter by status"
                options=filters.into_iter().map(|(v, l)| (v, view! { <span>{l}</span> }.into_any())).collect() />
            <span class="toolbar__spacer"></span>
            <Segmented value=view_mode label="View"
                options=vec![
                    (ViewMode::Grid, view! { <Icon icon=I::LayoutGrid class="icon icon--sm" /><span>"Grid"</span> }.into_any()),
                    (ViewMode::List, view! { <Icon icon=I::List class="icon icon--sm" /><span>"List"</span> }.into_any()),
                ] />
        </div>
    }
}
