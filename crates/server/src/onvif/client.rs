//! SOAP requests with WS-Security UsernameToken (password digest), timed
//! against the camera's own clock so skewed devices still accept them.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Duration, Utc};
use sha1::{Digest, Sha1};
use url::Url;

use super::xml;
use crate::httpc as http;

pub struct Client {
    pub url: Url,
    username: String,
    password: Option<String>,
    /// Camera time minus our time.
    clock_offset: Duration,
}

impl Client {
    pub fn new(url: Url, username: String, password: Option<String>) -> Self {
        Self { url, username, password, clock_offset: Duration::zero() }
    }

    /// Read the device clock (no authentication needed) to sign requests in its time.
    pub async fn sync_clock(&mut self) {
        let body = r#"<tds:GetSystemDateAndTime xmlns:tds="http://www.onvif.org/ver10/device/wsdl"/>"#;
        let Ok(reply) = self.raw(&self.url.clone(), "http://www.onvif.org/ver10/device/wsdl/GetSystemDateAndTime", body, false).await else { return };
        if let Some(t) = xml::device_utc_time(&reply) {
            self.clock_offset = t - Utc::now();
        }
    }

    /// Call `action` at `url`; returns the SOAP body XML or a readable error.
    pub async fn call(&self, url: &Url, action: &str, body: &str) -> Result<String, String> {
        self.raw(url, action, body, true).await
    }

    /// Like [`call`], with WS-Addressing `Action`/`To` headers (required by
    /// many devices on subscription endpoints).
    pub async fn call_addressed(&self, url: &Url, action: &str, body: &str) -> Result<String, String> {
        self.raw_with(url, action, body, true, true, None).await
    }

    /// [`call_addressed`] for a call the device may hold open up to `limit`.
    pub async fn call_addressed_within(&self, url: &Url, action: &str, body: &str, limit: std::time::Duration) -> Result<String, String> {
        self.raw_with(url, action, body, true, true, Some(limit)).await
    }

    async fn raw(&self, url: &Url, action: &str, body: &str, auth: bool) -> Result<String, String> {
        self.raw_with(url, action, body, auth, false, None).await
    }

    async fn raw_with(&self, url: &Url, action: &str, body: &str, auth: bool, addressed: bool, limit: Option<std::time::Duration>) -> Result<String, String> {
        let mut header = match (&self.password, auth) {
            (Some(p), true) => security_header(&self.username, p, Utc::now() + self.clock_offset, &nonce()),
            _ => String::new(),
        };
        if addressed {
            header.push_str(&format!(
                r#"<wsa:Action xmlns:wsa="http://www.w3.org/2005/08/addressing">{}</wsa:Action><wsa:To xmlns:wsa="http://www.w3.org/2005/08/addressing">{}</wsa:To>"#,
                xml::escape(action),
                xml::escape(url.as_str())
            ));
        }
        let envelope = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope"><s:Header>{header}</s:Header><s:Body>{body}</s:Body></s:Envelope>"#
        );
        // Errors name the call and carry the HTTP status: they end up in
        // the log, where "which request, what answer" is what helps.
        let op = operation(action);
        let reply = match limit {
            Some(l) => http::post_soap_within(url, action, &envelope, l).await,
            None => http::post_soap(url, action, &envelope).await,
        }
        .map_err(|e| format!("{op}: {e}"))?;
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
            return Err(format!("{op}: the camera rejected the ONVIF username or password (HTTP {})", reply.status));
        }
        let detail = if reason.is_empty() { snippet(&reply.body) } else { reason };
        Err(if detail.is_empty() { format!("{op}: the camera answered HTTP {}", reply.status) } else { format!("{op}: the camera answered HTTP {}: {detail}", reply.status) })
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
