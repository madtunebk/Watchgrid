use leptos::prelude::*;

/// Label + control + hint/error line.
#[component]
pub fn Field(
    #[prop(into)] label: Signal<&'static str>,
    #[prop(optional)] hint: &'static str,
    #[prop(optional, into)] error: Signal<Option<String>>,
    #[prop(optional)] required: bool,
    #[prop(optional)] optional: bool,
    children: Children,
) -> impl IntoView {
    view! {
        <label class="field" class:field--invalid=move || error.get().is_some()>
            <span class="field__label">
                {label}
                {required.then(|| view! { <span class="field__req" aria-hidden="true">"*"</span> })}
                {optional.then(|| view! { <span class="field__opt">"optional"</span> })}
            </span>
            {children()}
            {move || match error.get() {
                Some(e) => view! { <span class="field__error">{e}</span> }.into_any(),
                None if !hint.is_empty() => view! { <span class="field__hint">{hint}</span> }.into_any(),
                None => ().into_any(),
            }}
        </label>
    }
}

/// Titled group of fields.
#[component]
pub fn FormSection(
    title: &'static str,
    #[prop(optional)] description: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <section class="form-section">
            <header class="form-section__head">
                <h2 class="form-section__title">{title}</h2>
                {(!description.is_empty()).then(|| view! { <p class="form-section__desc">{description}</p> })}
            </header>
            <div class="form-section__body">{children()}</div>
        </section>
    }
}
