//! One notification's content: level dot, title, time and message. Used by
//! the bell's dropdown and the Notifications page.

use leptos::prelude::*;

use crate::api::{Notification, NotificationLevel};
use crate::format;
use crate::ui::{Dot, Tone};

fn tone(level: NotificationLevel) -> Tone {
    match level {
        NotificationLevel::Info => Tone::Accent,
        NotificationLevel::Success => Tone::Online,
        NotificationLevel::Warning => Tone::Warning,
        NotificationLevel::Error => Tone::Danger,
    }
}

#[component]
pub fn NotificationItem(notification: Notification) -> impl IntoView {
    let n = notification;
    view! {
        <Dot tone=tone(n.level) dim=n.read />
        <span class="notif__body">
            <span class="notif__top">
                <span class="notif__title truncate">{n.title}</span>
                <span class="notif__time">{format::relative(n.time)}</span>
            </span>
            <span class="notif__msg truncate">{n.message}</span>
        </span>
    }
}
