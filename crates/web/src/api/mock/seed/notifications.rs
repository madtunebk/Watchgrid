use chrono::{Duration, Utc};

use crate::api::{Notification, NotificationLevel};

pub fn all() -> Vec<Notification> {
    let note = |id: &str, level, title: &str, message: &str, minutes_ago: i64, read, link: &str| Notification {
        id: id.into(),
        level,
        title: title.into(),
        message: message.into(),
        time: Utc::now() - Duration::minutes(minutes_ago),
        read,
        link: Some(link.into()),
    };
    vec![
        note("n1", NotificationLevel::Warning, "Garage offline",
            "Lost RTSP connection to 192.168.1.31. Retrying.", 4, false, "/cameras/cam-garage"),
        note("n2", NotificationLevel::Info, "Person detected",
            "Front Door, 64 s clip recorded.", 12, false, "/events"),
        note("n3", NotificationLevel::Success, "Camera added",
            "Driveway is now online.", 180, true, "/cameras/cam-driveway"),
    ]
}
