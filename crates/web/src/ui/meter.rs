use leptos::prelude::*;

use super::status::Tone;

/// Horizontal usage bar. `value` is 0..=100.
#[component]
pub fn Meter(#[prop(into)] value: Signal<f32>, #[prop(optional, into)] tone: Option<Signal<Tone>>) -> impl IntoView {
    // Default colouring: accent, warning above 80 %, danger above 92 %.
    let tone = move || {
        tone.map(|t| t.get()).unwrap_or_else(|| match value.get() {
            v if v >= 92.0 => Tone::Danger,
            v if v >= 80.0 => Tone::Warning,
            _ => Tone::Accent,
        })
    };
    view! {
        <div class="meter" role="meter" aria-valuemin="0" aria-valuemax="100" aria-valuenow=move || value.get().round()>
            <div
                class=move || format!("meter__fill meter__fill--{}", tone().class())
                style:width=move || format!("{:.1}%", value.get().clamp(0.0, 100.0))
            ></div>
        </div>
    }
}
