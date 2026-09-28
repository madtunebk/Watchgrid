//! Events: the primary way to navigate recordings.

mod detail;
mod labels;
mod list;
mod mutations;
mod widgets;

pub use detail::EventDetailPage;
pub use labels::{css as kind_css, title};
pub use list::EventsPage;
pub use widgets::{EventChip, EventRow};
