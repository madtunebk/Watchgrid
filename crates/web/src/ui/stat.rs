use leptos::prelude::*;

use super::status::Tone;

/// Compact KPI tile: label, big value, optional detail line.
#[component]
pub fn Stat(
    label: &'static str,
    #[prop(into)] value: Signal<String>,
    #[prop(optional, into)] detail: Option<Signal<String>>,
    #[prop(optional, into)] tone: Option<Signal<Tone>>,
    /// Page with the details (the tile becomes a link).
    #[prop(optional)]
    href: Option<&'static str>,
    /// Extra content under the value (e.g. a meter).
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let body = view! {
        <div class="stat__label">{label}</div>
        <div class=move || match tone {
            Some(t) => format!("stat__value stat__value--{}", t.get().class()),
            None => "stat__value".to_string(),
        }>{value}</div>
        {children.map(|c| c())}
        {detail.map(|d| view! { <div class="stat__detail truncate">{d}</div> })}
    };
    match href {
        Some(href) => view! { <leptos_router::components::A href=href attr:class="stat stat--link">{body}</leptos_router::components::A> }.into_any(),
        None => view! { <div class="stat">{body}</div> }.into_any(),
    }
}
