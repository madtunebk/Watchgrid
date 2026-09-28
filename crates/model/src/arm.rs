//! Armed surveillance (Dashboard): the chosen cameras detect motion with
//! Watchgrid's own detection and record on it; disarming puts each camera
//! back as it was.

use serde::{Deserialize, Serialize};

use super::{Id, MotionSource, RecordingMode, Timestamp};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmState {
    /// Some camera is armed.
    pub armed: bool,
    /// When the first of them was armed.
    pub since: Option<Timestamp>,
    /// The armed cameras.
    pub cameras: Vec<Id>,
}

/// `armed: true` arms `cameras` (empty = every camera), adding to those
/// already armed; `armed: false` disarms all.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmInput {
    pub armed: bool,
    #[serde(default)]
    pub cameras: Vec<Id>,
}

/// What arming changes on a camera: motion on, Watchgrid's detection, and
/// recording on motion unless the camera already records (continuously or
/// on a schedule).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmSettings {
    pub motion_enabled: bool,
    pub source: MotionSource,
    pub mode: RecordingMode,
}

impl ArmSettings {
    /// What a camera with settings `self` gets while armed.
    pub fn armed(self) -> Self {
        let mode = match self.mode {
            RecordingMode::Disabled | RecordingMode::Manual => RecordingMode::Events,
            other => other,
        };
        Self { motion_enabled: true, source: MotionSource::Software, mode }
    }

    /// On disarm: the settings `before` arming, except where `self` (now)
    /// differs from what arming set: someone changed it meanwhile, and
    /// that change stays.
    pub fn disarmed(self, before: Self) -> Self {
        let armed = before.armed();
        Self {
            motion_enabled: if self.motion_enabled == armed.motion_enabled { before.motion_enabled } else { self.motion_enabled },
            source: if self.source == armed.source { before.source } else { self.source },
            mode: if self.mode == armed.mode { before.mode } else { self.mode },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFF: ArmSettings = ArmSettings { motion_enabled: false, source: MotionSource::Onvif, mode: RecordingMode::Manual };

    #[test]
    fn arming_turns_on_our_detection_and_recording_on_motion() {
        assert_eq!(OFF.armed(), ArmSettings { motion_enabled: true, source: MotionSource::Software, mode: RecordingMode::Events });
        let continuous = ArmSettings { mode: RecordingMode::Continuous, ..OFF };
        assert_eq!(continuous.armed().mode, RecordingMode::Continuous, "a camera that already records keeps its mode");
    }

    #[test]
    fn disarming_puts_back_what_arming_changed() {
        assert_eq!(OFF.armed().disarmed(OFF), OFF);
    }

    #[test]
    fn a_change_made_while_armed_stays() {
        let mut now = OFF.armed();
        now.mode = RecordingMode::Continuous; // changed by hand while armed
        let back = now.disarmed(OFF);
        assert_eq!(back.mode, RecordingMode::Continuous);
        assert_eq!((back.motion_enabled, back.source), (false, MotionSource::Onvif), "the rest goes back");
    }
}
