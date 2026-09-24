use leptos::prelude::*;

use super::draft::Draft;
use super::validate::Errors;
use crate::ui::form::{Field, FormSection, TextInput};

#[component]
pub fn IdentitySection(draft: Draft, errors: Signal<Errors>) -> impl IntoView {
    view! {
        <FormSection title="Camera" description="How this camera appears across Watchgrid.">
            <div class="form-grid">
                <Field label="Camera name" required=true error=Signal::derive(move || errors.get().name)>
                    <TextInput value=draft.name placeholder="Front Door" />
                </Field>
                <Field label="Location" optional=true>
                    <TextInput value=draft.location placeholder="Entrance" />
                </Field>
                <div class="form-grid__wide">
                    <Field label="Description" optional=true>
                        <TextInput value=draft.description placeholder="Doorbell camera above the front door" />
                    </Field>
                </div>
            </div>
        </FormSection>
    }
}
