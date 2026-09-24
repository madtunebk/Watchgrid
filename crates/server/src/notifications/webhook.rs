//! Webhook delivery: a JSON POST per notification (plain http:// —
//! Home Assistant, self-hosted ntfy/Gotify bridges and similar on the LAN).

use serde_json::json;
use watchgrid_model::Notification;

pub async fn send(url: &str, n: &Notification, nvr_name: &str) {
    let Ok(parsed) = url::Url::parse(url) else { return };
    let body = json!({
        "source": nvr_name,
        "id": n.id,
        "level": n.level,
        "title": n.title,
        "message": n.message,
        "time": n.time,
        "link": n.link,
    });
    match crate::httpc::post(&parsed, "application/json", &body.to_string()).await {
        Ok(r) if (200..300).contains(&r.status) => {}
        Ok(r) => tracing::warn!(status = r.status, "webhook answered with an error"),
        Err(e) => tracing::warn!("webhook failed: {e}"),
    }
}
