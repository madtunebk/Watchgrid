//! Settings: server-wide configuration, one section per concern.

mod page;
mod save;
mod sections;

pub use page::SettingsPage;

/// Display name of a settings section key (for breadcrumbs).
pub fn section_label(key: &str) -> Option<&'static str> {
    sections::ALL.iter().find(|s| s.key == key).map(|s| s.label)
}
