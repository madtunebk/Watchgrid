use leptos::prelude::*;

use super::status::Tone;

/// Small uppercase state label, e.g. ONLINE / REC / MOTION.
#[component]
pub fn Badge(
    #[prop(into)] tone: Signal<Tone>,
    #[prop(into)] label: Signal<&'static str>,
    /// Show a leading dot.
    #[prop(optional)]
    dot: bool,
    #[prop(optional, into)] pulse: Signal<bool>,
) -> impl IntoView {
    view! {
        <span class=move || format!("badge badge--{}", tone.get().class())>
            {dot.then(|| view! { <span class="badge__dot" class:pulse=pulse></span> })}
            {label}
        </span>
    }
}
