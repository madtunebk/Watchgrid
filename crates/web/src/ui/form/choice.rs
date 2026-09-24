//! Single-choice controls: radio cards, segmented buttons, slider.

use leptos::prelude::*;

/// One option of a choice control.
#[derive(Clone)]
pub struct Choice<T> {
    pub value: T,
    pub label: &'static str,
    pub description: &'static str,
    /// Small tag such as "Default" or "Future".
    pub tag: Option<&'static str>,
    /// When set, the option is shown but cannot be picked, with this reason.
    pub disabled: Option<&'static str>,
}

impl<T> Choice<T> {
    pub fn new(value: T, label: &'static str) -> Self {
        Self { value, label, description: "", tag: None, disabled: None }
    }
    pub fn describe(mut self, text: &'static str) -> Self {
        self.description = text;
        self
    }
    pub fn tag(mut self, tag: &'static str) -> Self {
        self.tag = Some(tag);
        self
    }
    pub fn disabled_because(mut self, reason: &'static str) -> Self {
        self.disabled = Some(reason);
        self
    }
}

/// Vertical list of selectable cards (radio semantics).
#[component]
pub fn RadioCards<T>(value: RwSignal<T>, options: Vec<Choice<T>>, name: &'static str) -> impl IntoView
where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    view! {
        <div class="radio-cards" role="radiogroup">
            {options.into_iter().map(|opt| {
                let v = opt.value;
                let disabled = opt.disabled.is_some();
                view! {
                    <label class="radio-card" class:radio-card--checked=move || value.get() == v class:radio-card--disabled=disabled
                        title=opt.disabled.unwrap_or("")>
                        <input type="radio" name=name class="radio-card__input" disabled=disabled
                            prop:checked=move || value.get() == v
                            on:change=move |_| value.set(v) />
                        <span class="radio-card__mark" aria-hidden="true"></span>
                        <span class="radio-card__text">
                            <span class="radio-card__label">
                                {opt.label}
                                {opt.tag.map(|t| view! { <span class="radio-card__tag">{t}</span> })}
                            </span>
                            {(!opt.description.is_empty()).then(|| view! { <span class="radio-card__desc">{opt.description}</span> })}
                            {opt.disabled.map(|r| view! { <span class="radio-card__why">{r}</span> })}
                        </span>
                    </label>
                }
            }).collect_view()}
        </div>
    }
}

/// Compact button group, e.g. Grid | List.
#[component]
pub fn Segmented<T>(value: RwSignal<T>, options: Vec<(T, AnyView)>, label: &'static str) -> impl IntoView
where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    view! {
        <div class="segmented" role="group" aria-label=label>
            {options.into_iter().map(|(v, content)| view! {
                <button type="button" class="segmented__btn" class:segmented__btn--active=move || value.get() == v
                    aria-pressed=move || (value.get() == v).to_string()
                    on:click=move |_| value.set(v)>
                    {content}
                </button>
            }).collect_view()}
        </div>
    }
}

/// 0–100 range slider with the value shown beside it.
#[component]
pub fn Slider(value: RwSignal<u8>, #[prop(optional, into)] disabled: Signal<bool>) -> impl IntoView {
    view! {
        <div class="slider">
            <input type="range" min="0" max="100" step="1" class="slider__input" disabled=disabled
                style:--pct=move || format!("{}%", value.get())
                prop:value=move || value.get().to_string()
                on:input=move |ev| value.set(event_target_value(&ev).parse().unwrap_or(0)) />
            <span class="slider__value">{move || format!("{}%", value.get())}</span>
        </div>
    }
}
