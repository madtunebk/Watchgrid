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
