//! Webhook delivery: a JSON POST per notification, over http:// or https://
//! (Home Assistant, ntfy/Gotify bridges and similar).

use std::time::Duration;

use serde_json::json;
use watchgrid_model::Notification;

/// Deliver one notification; the outcome is logged, and returned for tests.
pub async fn send(url: &str, n: &Notification, nvr_name: &str) -> Result<u16, String> {
    let parsed = url::Url::parse(url).map_err(|_| "invalid webhook address".to_string())?;
    let body = json!({
        "source": nvr_name,
        "id": n.id,
        "level": n.level,
        "title": n.title,
        "message": n.message,
        "time": n.time,
        "link": n.link,
    });
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("the webhook must be an http(s) address".into());
    }
    let sent = crate::https::client().post(parsed).timeout(Duration::from_secs(10)).header(reqwest::header::CONTENT_TYPE, "application/json").body(body.to_string()).send().await;
    match sent {
        Ok(r) if r.status().is_success() => Ok(r.status().as_u16()),
        Ok(r) => {
            tracing::warn!(status = r.status().as_u16(), "webhook answered with an error");
            Err(format!("the webhook answered HTTP {}", r.status()))
        }
        // Without the URL: webhook addresses often carry a secret token.
        Err(e) => {
            let e = e.without_url();
            let mut msg = format!("cannot reach the webhook: {e}");
            let mut source = std::error::Error::source(&e);
            while let Some(s) = source {
                msg.push_str(&format!(": {s}"));
                source = s.source();
            }
            tracing::warn!("webhook failed: {msg}");
            Err(msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use watchgrid_model::NotificationLevel;

    use super::*;

    fn note() -> Notification {
        Notification { id: "n1".into(), level: NotificationLevel::Info, title: "Test".into(), message: "hello".into(), time: chrono::Utc::now(), read: false, link: None, camera_id: None }
    }

    /// One-request server answering `status`; returns what it received.
    async fn server(status: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = vec![0; 8192];
            let mut got = String::new();
            while !got.contains("\"message\"") {
                let n = s.read(&mut buf).await.unwrap();
                if n == 0 { break; }
                got.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            s.write_all(format!("HTTP/1.1 {status}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n").as_bytes()).await.unwrap();
            got
        });
        (format!("http://{addr}/api/webhook/secret-token"), task)
    }

    #[tokio::test]
    async fn delivers_json_and_reports_refusals_without_the_address() {
        let (url, got) = server("200 OK").await;
        assert_eq!(send(&url, &note(), "NVR").await, Ok(200));
        let request = got.await.unwrap();
        assert!(request.starts_with("POST /api/webhook/secret-token"), "{request}");
        assert!(request.contains("application/json") && request.contains("\"source\":\"NVR\""), "{request}");

        let (url, _) = server("401 Unauthorized").await;
        assert_eq!(send(&url, &note(), "NVR").await, Err("the webhook answered HTTP 401 Unauthorized".into()));

        let err = send("http://127.0.0.1:9/api/webhook/secret-token", &note(), "NVR").await.unwrap_err();
        assert!(err.starts_with("cannot reach the webhook"), "{err}");
        assert!(!err.contains("secret-token"), "the address stays out of errors: {err}");
        assert!(send("ftp://x/y", &note(), "NVR").await.is_err());
    }
}
