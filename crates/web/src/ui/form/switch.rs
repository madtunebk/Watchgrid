use leptos::prelude::*;

/// On/off toggle with label and optional description.
#[component]
pub fn Switch(
    checked: RwSignal<bool>,
    label: &'static str,
    #[prop(optional)] description: &'static str,
    #[prop(optional, into)] disabled: Signal<bool>,
) -> impl IntoView {
    view! {
        <label class="switch" class:switch--disabled=disabled>
            <input
                type="checkbox"
                role="switch"
                class="switch__input"
                disabled=disabled
                bind:checked=checked
            />
            <span class="switch__track" aria-hidden="true"><span class="switch__thumb"></span></span>
            <span class="switch__text">
                <span class="switch__label">{label}</span>
                {(!description.is_empty()).then(|| view! { <span class="switch__desc">{description}</span> })}
            </span>
        </label>
    }
}
