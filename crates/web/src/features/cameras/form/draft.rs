//! Editable form state for a camera, one signal per field.

use leptos::prelude::*;

use crate::api::{CameraInput, MotionSettings, OnvifConfig, RecordingSettings};

#[derive(Clone, Copy)]
pub struct Draft {
    pub name: RwSignal<String>,
    pub description: RwSignal<String>,
    pub location: RwSignal<String>,
    pub host: RwSignal<String>,
    pub username: RwSignal<String>,
    /// Empty on edit means "keep the stored password".
    pub password: RwSignal<String>,
    pub main_url: RwSignal<String>,
    pub sub_url: RwSignal<String>,
    pub onvif_enabled: RwSignal<bool>,
    pub onvif_url: RwSignal<String>,
    pub onvif_username: RwSignal<String>,
    pub onvif_password: RwSignal<String>,
    /// Settings not edited on this form, carried through unchanged.
    rest: StoredValue<(bool, RecordingSettings, MotionSettings)>,
}

impl Draft {
    pub fn blank() -> Self {
        Self::from_input(&CameraInput {
            name: String::new(),
            description: String::new(),
            location: String::new(),
            enabled: true,
            host: String::new(),
            username: "admin".into(),
            password: None,
            main_stream_url: String::new(),
            sub_stream_url: None,
            onvif: None,
            recording: RecordingSettings::default(),
            motion: MotionSettings::default(),
        })
    }

    pub fn from_input(i: &CameraInput) -> Self {
        let onvif = i.onvif.clone();
        Self {
            name: RwSignal::new(i.name.clone()),
            description: RwSignal::new(i.description.clone()),
            location: RwSignal::new(i.location.clone()),
            host: RwSignal::new(i.host.clone()),
            username: RwSignal::new(i.username.clone()),
            password: RwSignal::new(String::new()),
            main_url: RwSignal::new(i.main_stream_url.clone()),
            sub_url: RwSignal::new(i.sub_stream_url.clone().unwrap_or_default()),
            onvif_enabled: RwSignal::new(onvif.is_some()),
            onvif_url: RwSignal::new(onvif.as_ref().map(|o| o.url.clone()).unwrap_or_default()),
            onvif_username: RwSignal::new(onvif.as_ref().map(|o| o.username.clone()).unwrap_or_default()),
            onvif_password: RwSignal::new(String::new()),
            rest: StoredValue::new((i.enabled, i.recording.clone(), i.motion.clone())),
        }
    }

    /// Subscribe the current reactive scope to every field.
    pub fn track(&self) {
        for s in [self.name, self.description, self.location, self.host, self.username, self.password, self.main_url, self.sub_url, self.onvif_url, self.onvif_username, self.onvif_password] {
            s.track();
        }
        self.onvif_enabled.track();
    }

    pub fn to_input(&self) -> CameraInput {
        let (enabled, recording, mut motion) = self.rest.get_value();
        let non_empty = |s: String| {
            let t = s.trim().to_string();
            (!t.is_empty()).then_some(t)
        };
        let onvif = self.onvif_enabled.get_untracked().then(|| OnvifConfig {
            url: self.onvif_url.get_untracked().trim().into(),
            username: self.onvif_username.get_untracked().trim().into(),
            password: non_empty(self.onvif_password.get_untracked()),
        });
        // Without ONVIF the camera cannot report motion itself.
        if onvif.is_none() && motion.source == crate::api::MotionSource::Onvif {
            motion.source = crate::api::MotionSource::Software;
        }
        CameraInput {
            name: self.name.get_untracked().trim().into(),
            description: self.description.get_untracked().trim().into(),
            location: self.location.get_untracked().trim().into(),
            enabled,
            host: self.host.get_untracked().trim().into(),
            username: self.username.get_untracked().trim().into(),
            password: non_empty(self.password.get_untracked()),
            main_stream_url: self.main_url.get_untracked().trim().into(),
            sub_stream_url: non_empty(self.sub_url.get_untracked()),
            onvif,
            recording,
            motion,
        }
    }
}
