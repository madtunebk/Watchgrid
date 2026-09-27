//! Camera management: list, add/edit form, details, and the camera widgets
//! other features reuse.

mod details;
mod form;
pub(crate) mod labels;
mod list;
mod mutations;
mod widgets;

pub use details::CameraDetailsPage;
pub use form::CameraFormPage;
pub use list::CamerasPage;
pub use widgets::{CameraCard, CameraPreview, NoCameras, PtzPad, RecordButton};

/// Key for keyed lists: a refresh rebuilds only the cameras that changed,
/// so unchanged cards keep their preview and rows keep an open menu.
pub fn render_key(c: &crate::api::Camera) -> String {
    format!("{c:?}")
}
