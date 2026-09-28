//! Armed surveillance: the chosen cameras detect motion with Watchgrid's own
//! detection and record on it; disarming puts each camera back as it was.
//! Reached from the header, left of the bell, on every page.

mod dialog;
mod menu;

pub use menu::ArmMenu;
