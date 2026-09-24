//! Application frame: sidebar, header, navigation and routing fallbacks.

mod header;
mod layout;
pub(crate) mod nav;
mod not_found;
mod sidebar;

pub use layout::Shell;
pub use not_found::NotFound;
