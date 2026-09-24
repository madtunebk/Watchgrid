//! Rendering of query results: loading skeleton, error box, or content.

use leptos::prelude::*;

use super::icons::{I, Icon};
use crate::api::{ApiError, ApiResult};

/// Render `resource` with `content`, showing `skeleton` while the first load
/// is in flight and an error box if it fails. Refetches keep the previous
/// content on screen, so there is no flicker.
pub fn async_view<T, V>(
    resource: LocalResource<ApiResult<T>>,
    skeleton: impl Fn() -> AnyView + Send + Sync + 'static,
    content: impl Fn(T) -> V + Send + Sync + 'static,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    V: IntoView + 'static,
{
    move || match resource.get() {
        None => skeleton(),
        Some(Ok(data)) => content(data).into_any(),
        Some(Err(error)) => view! { <ErrorBox error /> }.into_any(),
    }
}

#[component]
pub fn ErrorBox(error: ApiError) -> impl IntoView {
    view! {
        <div class="error-box" role="alert">
            <Icon icon=I::TriangleAlert />
            <span>{format!("Couldn't load data: {error}")}</span>
        </div>
    }
}

/// Grey placeholder block(s) shown while loading.
#[component]
pub fn Skeleton(#[prop(default = 1)] lines: usize, #[prop(default = "1rem")] height: &'static str) -> impl IntoView {
    view! {
        <div class="skeleton-group" aria-busy="true">
            {(0..lines).map(|_| view! { <div class="skeleton" style:height=height></div> }).collect_view()}
        </div>
    }
}
