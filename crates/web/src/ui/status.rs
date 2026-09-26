use leptos::prelude::*;

/// Colour roles for status indicators. Online, streaming, motion and
/// recording each have their own colour and are never merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Online,
    Offline,
    Stream,
    Motion,
    Recording,
    Warning,
    Danger,
    Accent,
}

impl Tone {
    pub fn class(self) -> &'static str {
        match self {
            Tone::Online => "online",
            Tone::Offline => "offline",
            Tone::Stream => "stream",
            Tone::Motion => "motion",
            Tone::Recording => "recording",
            Tone::Warning => "warning",
            Tone::Danger => "danger",
            Tone::Accent => "accent",
        }
    }
}

#[component]
pub fn Dot(
    #[prop(into)] tone: Signal<Tone>,
    #[prop(optional, into)] pulse: Signal<bool>,
    #[prop(optional, into)] dim: Signal<bool>,
) -> impl IntoView {
    view! {
        <span
            aria-hidden="true"
            class=move || format!("dot dot--{}", tone.get().class())
            class:pulse=pulse
            class:dot--dim=dim
        ></span>
    }
}
