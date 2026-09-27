//! SOAP requests with WS-Security UsernameToken (password digest), timed
//! against the camera's own clock so skewed devices still accept them, and
//! HTTP Digest for cameras that ask for it.

use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Duration, Utc};
use sha1::{Digest, Sha1};
use url::Url;

use super::digest::{self, Challenge};
use super::refusal::{self, Refusal};
use super::xml;
use crate::httpc as http;

pub struct Client {
    pub url: Url,
    username: String,
    password: Option<String>,
    /// Camera time minus our time.
    clock_offset: Duration,
    clock: Clock,
    /// The camera's Digest challenge and how many requests answered it
    /// (sent with every request once the camera asked for it).
    digest: Mutex<Option<(Challenge, u32)>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Clock {
    NotAsked,
    Read,
    Unreadable,
}

impl Client {
    pub fn new(url: Url, username: String, password: Option<String>) -> Self {
        Self { url, username, password, clock_offset: Duration::zero(), clock: Clock::NotAsked, digest: Mutex::new(None) }
    }

    /// Read the device clock (no authentication needed) to sign requests in
    /// its time. If that fails, a later refusal says so.
    pub async fn sync_clock(&mut self) {
        let body = r#"<tds:GetSystemDateAndTime xmlns:tds="http://www.onvif.org/ver10/device/wsdl"/>"#;
        let reply = self.raw(&self.url.clone(), "http://www.onvif.org/ver10/device/wsdl/GetSystemDateAndTime", body, false).await;
        match reply.ok().as_deref().and_then(xml::device_utc_time) {
            Some(t) => {
                self.clock_offset = t - Utc::now();
                self.clock = Clock::Read;
            }
            None => self.clock = Clock::Unreadable,
        }
    }

    /// Call `action` at `url`; returns the SOAP body XML or a readable error.
    pub async fn call(&self, url: &Url, action: &str, body: &str) -> Result<String, String> {
        self.raw(url, action, body, true).await
    }

    /// A call to a subscription: WS-Addressing `Action`/`To` headers (many
    /// devices require them there) plus the reference parameters the
    /// device gave the subscription. `limit`: how long it may hold the call.
    pub async fn call_endpoint(&self, to: &Url, reference: &[String], action: &str, body: &str, limit: Option<std::time::Duration>) -> Result<String, String> {
        self.raw_with(to, action, body, true, Some(reference), limit).await
    }

    async fn raw(&self, url: &Url, action: &str, body: &str, auth: bool) -> Result<String, String> {
        self.raw_with(url, action, body, auth, None, None).await
    }

    async fn raw_with(&self, url: &Url, action: &str, body: &str, auth: bool, reference: Option<&[String]>, limit: Option<std::time::Duration>) -> Result<String, String> {
        let mut header = match (&self.password, auth) {
            (Some(p), true) => security_header(&self.username, p, Utc::now() + self.clock_offset, &nonce()),
            _ => String::new(),
        };
        if let Some(parameters) = reference {
            header.push_str(&format!(
                r#"<wsa:Action xmlns:wsa="http://www.w3.org/2005/08/addressing">{}</wsa:Action><wsa:To xmlns:wsa="http://www.w3.org/2005/08/addressing">{}</wsa:To>"#,
                xml::escape(action),
                xml::escape(url.as_str())
            ));
            header.extend(parameters.iter().map(String::as_str));
        }
        let envelope = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope"><s:Header>{header}</s:Header><s:Body>{body}</s:Body></s:Envelope>"#
        );
        // Errors name the call and carry the HTTP status: they end up in
        // the log, where "which request, what answer" is what helps.
        let op = operation(action);
        let envelope = envelope.as_str();
        let send = |authorization: Option<String>| async move { http::post_soap(url, action, envelope, authorization.as_deref(), limit).await.map_err(|e| format!("{op}: {e}")) };
        let mut reply = send(self.digest_answer(url, auth)).await?;
        // A camera that wants HTTP Digest says so in its 401 (also when the
        // nonce we answered went stale): answer that challenge, once.
        if reply.status == 401
            && auth
            && self.password.is_some()
            && let Some(challenge) = reply.challenges.iter().find_map(|c| digest::parse(c))
        {
            *self.digest.lock().expect("digest lock") = Some((challenge, 0));
            reply = send(self.digest_answer(url, auth)).await?;
        }
        if reply.status == 200 {
            // A fault is a refusal whatever the HTTP status says.
            if xml::is_fault(&reply.body) {
                let reason = xml::fault_reason(&reply.body).unwrap_or_else(|| "no reason given".into());
                return Err(format!("{op}: the camera answered with a SOAP fault: {reason}"));
            }
            return Ok(reply.body);
        }
        let reason = xml::fault_reason(&reply.body).unwrap_or_default();
        if reply.status == 401 || reason.to_ascii_lowercase().contains("not authorized") || reason.contains("NotAuthorized") {
            let why = refusal::explain(&Refusal {
                status: reply.status,
                challenges: &reply.challenges,
                body: &reply.body,
                has_password: self.password.is_some(),
                clock_unreadable: self.clock == Clock::Unreadable,
            });
            return Err(format!("{op}: {why}"));
        }
        let detail = if reason.is_empty() { snippet(&reply.body) } else { reason };
        Err(if detail.is_empty() { format!("{op}: the camera answered HTTP {}", reply.status) } else { format!("{op}: the camera answered HTTP {}: {detail}", reply.status) })
    }
}

impl Client {
    /// The Digest `Authorization` for the next request, once the camera asked.
    fn digest_answer(&self, url: &Url, auth: bool) -> Option<String> {
        let password = self.password.as_deref().filter(|_| auth)?;
        let mut guard = self.digest.lock().expect("digest lock");
        let (challenge, count) = guard.as_mut()?;
        *count += 1;
        let cnonce: String = nonce().iter().map(|b| format!("{b:02x}")).collect();
        Some(challenge.authorization(&self.username, password, "POST", &http::request_uri(url), *count, &cnonce))
    }
}

/// `…/PullPointSubscription/PullMessagesRequest` → `PullMessages`.
fn operation(action: &str) -> &str {
    let last = action.rsplit('/').next().unwrap_or(action);
    last.strip_suffix("Request").unwrap_or(last)
}

/// The start of a non-SOAP answer (Tapo answers some refusals with JSON,
/// e.g. `{"error_code":-40210}`), on one line.
fn snippet(body: &str) -> String {
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(160).collect()
}

fn nonce() -> [u8; 16] {
    use aes_gcm::aead::OsRng;
    use aes_gcm::aead::rand_core::RngCore;
    let mut n = [0u8; 16];
    OsRng.fill_bytes(&mut n);
    n
}

/// WS-Security UsernameToken with PasswordDigest = Base64(SHA1(nonce + created + password)).
fn security_header(username: &str, password: &str, created: DateTime<Utc>, nonce: &[u8]) -> String {
    let created = created.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    let mut sha = Sha1::new();
    sha.update(nonce);
    sha.update(created.as_bytes());
    sha.update(password.as_bytes());
    let digest = B64.encode(sha.finalize());
    format!(
        r#"<wsse:Security s:mustUnderstand="1" xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd" xmlns:wsu="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd"><wsse:UsernameToken><wsse:Username>{}</wsse:Username><wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{digest}</wsse:Password><wsse:Nonce EncodingType="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary">{}</wsse:Nonce><wsu:Created>{created}</wsu:Created></wsse:UsernameToken></wsse:Security>"#,
        xml::escape(username),
        B64.encode(nonce)
    )
}

#[cfg(test)]
mod tests {

    #[test]
    fn errors_name_the_call_and_keep_the_camera_s_words() {
        assert_eq!(super::operation("http://www.onvif.org/ver10/events/wsdl/PullPointSubscription/PullMessagesRequest"), "PullMessages");
        assert_eq!(super::operation("http://docs.oasis-open.org/wsn/bw-2/SubscriptionManager/RenewRequest"), "Renew");
        assert_eq!(super::snippet("{\n  \"error_code\": -40210\n}"), "{ \"error_code\": -40210 }");
    }

    use super::*;

    /// A camera that wants HTTP Digest: 401 with a challenge unless the
    /// request carries a Digest answer. Counts the connections it gets.
    async fn digest_camera() -> (Url, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!("http://{}/onvif/device_service", listener.local_addr().unwrap())).unwrap();
        let hits = std::sync::Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                counter.fetch_add(1, Ordering::SeqCst);
                let mut buf = vec![0u8; 16 * 1024];
                let n = sock.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let answered = request.contains(r#"Authorization: Digest username="admin", realm="cam""#) && request.contains(r#"uri="/onvif/device_service""#);
                let reply = if answered {
                    "HTTP/1.1 200 OK\r\nContent-Length: 38\r\n\r\n<Envelope><Body><Ok/></Body></Envelope>".to_string()
                } else {
                    "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Digest realm=\"cam\", nonce=\"abc\", qop=\"auth\"\r\nContent-Length: 0\r\n\r\n".to_string()
                };
                sock.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        (url, hits)
    }

    #[tokio::test]
    async fn a_digest_camera_is_answered_and_then_asked_straight_away() {
        use std::sync::atomic::Ordering;
        let (url, hits) = digest_camera().await;
        let client = Client::new(url.clone(), "admin".into(), Some("pw".into()));
        let body = client.call(&url, "http://www.onvif.org/ver10/device/wsdl/GetDeviceInformation", "<x/>").await.unwrap();
        assert!(body.contains("<Ok/>"));
        assert_eq!(hits.load(Ordering::SeqCst), 2, "challenged once, then answered");
        client.call(&url, "http://www.onvif.org/ver10/device/wsdl/GetDeviceInformation", "<x/>").await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 3, "the next call answers at once");
    }

    #[tokio::test]
    async fn without_a_password_a_digest_camera_says_so() {
        let (url, _) = digest_camera().await;
        let client = Client::new(url.clone(), "admin".into(), None);
        let err = client.call(&url, "http://www.onvif.org/ver10/media/wsdl/GetProfiles", "<x/>").await.unwrap_err();
        assert_eq!(err, "GetProfiles: the camera wants a login, and no ONVIF password is saved for this camera");
    }

    #[test]
    fn digest_matches_the_ws_security_example() {
        // Values from the OASIS UsernameToken profile style: fixed inputs, known SHA-1.
        let created: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let h = security_header("admin", "secret", created, &[0u8; 16]);
        let mut sha = Sha1::new();
        sha.update([0u8; 16]);
        sha.update(b"2026-01-01T00:00:00.000Z");
        sha.update(b"secret");
        assert!(h.contains(&B64.encode(sha.finalize())));
        assert!(h.contains("<wsse:Username>admin</wsse:Username>") && !h.contains(">secret<"), "the password itself is never sent");
    }
}
