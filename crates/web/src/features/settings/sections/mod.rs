//! One module per settings section.

mod advanced;
mod auth;
mod exports;
mod general;
mod network;
mod notifications;
mod recording;
mod storage;

use crate::ui::I;

pub struct Section {
    pub key: &'static str,
    pub label: &'static str,
    pub icon: I,
}

pub const ALL: &[Section] = &[
    Section { key: "general", label: "General", icon: I::Settings },
    Section { key: "recording", label: "Recording", icon: I::RecordDot },
    Section { key: "storage", label: "Storage", icon: I::HardDrive },
    Section { key: "network", label: "Network", icon: I::Wifi },
    Section { key: "authentication", label: "Authentication", icon: I::Lock },
    Section { key: "notifications", label: "Notifications", icon: I::Bell },
    Section { key: "exports", label: "Export destinations", icon: I::Cloud },
    Section { key: "advanced", label: "Advanced", icon: I::Cpu },
];

pub use advanced::AdvancedSection;
pub use auth::AuthSection;
pub use exports::ExportsSection;
pub use general::GeneralSection;
pub use network::NetworkSection;
pub use notifications::NotificationsSection;
pub use recording::RecordingSection;
pub use storage::StorageSection;
