use leptos::prelude::*;

/// Tiny line chart of recent values (0..=max), newest on the right.
#[component]
pub fn Sparkline(#[prop(into)] values: Signal<Vec<f32>>, #[prop(default = 100.0)] max: f32, #[prop(default = "accent")] tone: &'static str) -> impl IntoView {
    const W: f32 = 120.0;
    const H: f32 = 32.0;
    let points = move || {
        let v = values.get();
        let n = v.len().max(2) as f32 - 1.0;
        v.iter()
            .enumerate()
            .map(|(i, x)| format!("{:.1},{:.1}", i as f32 / n * W, H - (x / max).clamp(0.0, 1.0) * (H - 2.0) - 1.0))
            .collect::<Vec<_>>()
            .join(" ")
    };
    view! {
        <svg class=format!("sparkline sparkline--{tone}") viewBox=format!("0 0 {W} {H}") preserveAspectRatio="none" aria-hidden="true">
            <polyline points=points fill="none" stroke="currentColor" stroke-width="1.5" vector-effect="non-scaling-stroke"></polyline>
        </svg>
    }
}
