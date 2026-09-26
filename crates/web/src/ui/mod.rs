//! Generic UI building blocks. Nothing here knows about cameras, events or
//! any other NVR domain; feature modules compose these.

mod async_view;
mod badge;
mod dialog;
mod draft;
mod empty;
pub mod clipboard;
pub mod form;
pub mod fullscreen;
pub mod snapshot;
mod icons;
mod meter;
mod on_screen;
mod page;
mod pager;
mod panel;
mod popover;
mod save_bar;
mod selection;
mod sparkline;
mod stat;
mod status;
mod tabs;

pub use async_view::{ErrorBox, Skeleton, async_view};
pub use badge::Badge;
pub use dialog::ConfirmDialog;
pub use draft::follow_server;
pub use empty::EmptyState;
pub use icons::{I, Icon};
pub use meter::Meter;
pub use on_screen::use_on_screen;
pub use page::Page;
pub use pager::Pager;
pub use panel::Panel;
pub use popover::Popover;
pub use save_bar::{SaveBar, SaveState};
pub use selection::{ResultNote, SelectCell, Selection, SelectionBar, keep_only_shown};
pub use sparkline::Sparkline;
pub use stat::Stat;
pub use status::{Dot, Tone};
pub use tabs::{Tab, TabNav};
