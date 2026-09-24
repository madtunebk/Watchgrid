use leptos::prelude::*;

#[component]
pub fn TextInput(
    value: RwSignal<String>,
    #[prop(optional, into)] placeholder: Signal<String>,
    /// "text", "password", "url"…
    #[prop(default = "text")]
    kind: &'static str,
    #[prop(optional)] mono: bool,
    #[prop(default = "off")] autocomplete: &'static str,
    #[prop(optional, into)] disabled: Signal<bool>,
) -> impl IntoView {
    view! {
        <input
            class="input"
            class:mono=mono
            type=kind
            placeholder=placeholder
            autocomplete=autocomplete
            spellcheck="false"
            disabled=disabled
            bind:value=value
        />
    }
}

/// Whole-number input with an optional unit suffix ("seconds").
#[component]
pub fn NumberInput(
    value: RwSignal<u32>,
    #[prop(default = 0)] min: u32,
    #[prop(default = u32::MAX)] max: u32,
    #[prop(optional)] suffix: &'static str,
    #[prop(optional, into)] disabled: Signal<bool>,
) -> impl IntoView {
    view! {
        <span class="number-input">
            <input
                class="input mono"
                type="number"
                inputmode="numeric"
                min=min
                max=max
                disabled=disabled
                prop:value=move || value.get().to_string()
                on:change=move |ev| {
                    let parsed = event_target_value(&ev).trim().parse::<u32>().unwrap_or(min);
                    value.set(parsed.clamp(min, max));
                }
            />
            {(!suffix.is_empty()).then(|| view! { <span class="number-input__suffix">{suffix}</span> })}
        </span>
    }
}
