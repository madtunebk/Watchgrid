//! Drafts of forms that edit server data.

use leptos::prelude::*;

/// Keep an untouched form in step with the server: when the saved values
/// change (another tab, another session) and the form still shows the old
/// ones, `revert` loads the new ones. A form with local edits is left alone.
pub fn follow_server<T: PartialEq + Clone + Send + Sync + 'static>(
    draft: impl Fn() -> T + Send + Sync + 'static,
    saved: impl Fn() -> T + Send + Sync + 'static,
    revert: Callback<()>,
) {
    let last = StoredValue::new(untrack(&saved));
    Effect::new(move |_| {
        let now = saved();
        let before = last.get_value();
        if now != before {
            if untrack(&draft) == before {
                revert.run(());
            }
            last.set_value(now);
        }
    });
}
