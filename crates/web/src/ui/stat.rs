use leptos::prelude::*;

use super::status::Tone;

/// Compact KPI tile: label, big value, optional detail line.
#[component]
pub fn Stat(
    label: &'static str,
    #[prop(into)] value: Signal<String>,
    #[prop(optional, into)] detail: Option<Signal<String>>,
    #[prop(optional, into)] tone: Option<Signal<Tone>>,
    /// Extra content under the value (e.g. a meter).
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="stat">
            <div class="stat__label">{label}</div>
            <div class=move || match tone {
                Some(t) => format!("stat__value stat__value--{}", t.get().class()),
                None => "stat__value".to_string(),
            }>{value}</div>
            {children.map(|c| c())}
            {detail.map(|d| view! { <div class="stat__detail truncate">{d}</div> })}
        </div>
    }
}
