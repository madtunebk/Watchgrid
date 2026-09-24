use chrono::{Duration, Utc};

use crate::api::{Role, Session, User};

pub fn users() -> Vec<User> {
    vec![
        User { id: "u1".into(), username: "admin".into(), role: Role::Admin, last_login: Some(Utc::now() - Duration::minutes(20)) },
        User { id: "u2".into(), username: "family".into(), role: Role::Viewer, last_login: Some(Utc::now() - Duration::days(2)) },
    ]
}

pub fn sessions() -> Vec<Session> {
    let s = |id: &str, user: &str, client: &str, address: &str, minutes: i64, current| Session {
        id: id.into(),
        username: user.into(),
        client: client.into(),
        address: address.into(),
        last_seen: Utc::now() - Duration::minutes(minutes),
        current,
    };
    vec![
        s("s1", "admin", "This browser", "192.168.1.10", 0, true),
        s("s2", "family", "Safari on iPhone", "192.168.1.54", 180, false),
        s("s3", "admin", "Firefox on Android", "10.8.0.6 (VPN)", 2_880, false),
    ]
}
