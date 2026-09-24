use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

use crate::api::{self, Topic, use_query};
use crate::shell::nav::crumbs;
use crate::ui::{I, Icon};

/// Trail derived from the current path.
#[component]
pub fn Breadcrumbs() -> impl IntoView {
    let pathname = use_location().pathname;
    // Show a camera's name instead of the generic "Camera" crumb.
    let cameras = use_query(Topic::Cameras, None, api::get_cameras);
    let camera_name = move |href: &str| {
        let id = href.strip_prefix("/cameras/")?;
        cameras.get()?.ok()?.into_iter().find(|c| c.id == id).map(|c| c.name)
    };
    view! {
        <nav class="crumbs" aria-label="Breadcrumb">
            {move || {
                let trail = crumbs(&pathname.get());
                let last = trail.len().saturating_sub(1);
                trail
                    .into_iter()
                    .enumerate()
                    .map(|(i, (label, href))| {
                        let label = if label == "Camera" { camera_name(&href).unwrap_or(label) } else { label };
                        view! {
                            <span class="crumbs__item">
                                {(i > 0).then(|| view! { <span class="crumbs__sep"><Icon icon=I::ChevronRight class="icon icon--sm" /></span> })}
                                {if i == last {
                                    view! { <span class="crumbs__current truncate" aria-current="page">{label}</span> }.into_any()
                                } else {
                                    view! { <A href=href attr:class="truncate">{label}</A> }.into_any()
                                }}
                            </span>
                        }
                    })
                    .collect_view()
            }}
        </nav>
    }
}
