use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <div class="error-page">
            <div class="error-page__code">"404"</div>
            <h1 class="error-page__title">"Page not found"</h1>
            <p class="error-page__text">"The page you requested does not exist."</p>
            <A href="/" attr:class="btn btn--sm btn--secondary">"Back to dashboard"</A>
        </div>
    }
}
