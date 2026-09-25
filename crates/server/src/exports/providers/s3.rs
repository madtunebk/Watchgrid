//! S3-compatible object storage (AWS, MinIO, Wasabi, Backblaze B2…).
//! Path-style requests, SigV4 with an unsigned streamed payload.

use std::path::Path;
use std::sync::RwLock;

use chrono::Utc;
use url::Url;

use super::sigv4::{self, Credentials, UNSIGNED_PAYLOAD};
use super::{TargetConfig, client, failure, file_body, net_error};

pub struct S3 {
    endpoint: Url,
    bucket: String,
    prefix: String,
    access_key: String,
    secret_key: String,
    /// Guessed from the endpoint; corrected if the server says otherwise.
    region: RwLock<String>,
}

impl S3 {
    pub fn new(c: &TargetConfig<'_>) -> Result<Self, String> {
        let endpoint = Url::parse(c.endpoint.trim().trim_end_matches('/')).map_err(|_| "enter the S3 endpoint, e.g. https://minio.local:9000".to_string())?;
        if !matches!(endpoint.scheme(), "http" | "https") || endpoint.host_str().is_none() {
            return Err("the S3 endpoint must be an http(s) URL".into());
        }
        let location = c.location.trim().trim_matches('/');
        let (bucket, prefix) = location.split_once('/').unwrap_or((location, ""));
        if bucket.is_empty() {
            return Err("enter the bucket (optionally bucket/prefix)".into());
        }
        if c.username.trim().is_empty() || c.secret.is_empty() {
            return Err("enter the access key and secret key".into());
        }
        Ok(Self {
            region: RwLock::new(region_of(endpoint.host_str().unwrap_or(""))),
            endpoint,
            bucket: bucket.to_string(),
            prefix: prefix.trim_matches('/').to_string(),
            access_key: c.username.trim().to_string(),
            secret_key: c.secret.to_string(),
        })
    }

    fn key(&self, name: &str) -> String {
        if self.prefix.is_empty() { name.to_string() } else { format!("{}/{name}", self.prefix) }
    }

    fn request(&self, method: reqwest::Method, key: &str, payload_hash: &str) -> reqwest::RequestBuilder {
        let region = self.region.read().expect("region lock").clone();
        let path = sigv4::encode_path(&format!("{}/{}/{key}", self.endpoint.path().trim_end_matches('/'), self.bucket));
        let host = match self.endpoint.port() {
            Some(p) => format!("{}:{p}", self.endpoint.host_str().unwrap_or("")),
            None => self.endpoint.host_str().unwrap_or("").to_string(),
        };
        let url = format!("{}://{host}{path}", self.endpoint.scheme());
        let creds = Credentials { access_key: &self.access_key, secret_key: &self.secret_key, region: &region };
        let mut req = client().request(method.clone(), url);
        for (k, v) in sigv4::sign(&creds, method.as_str(), &host, &path, "", payload_hash, Utc::now()) {
            req = req.header(k, v);
        }
        req
    }

    /// Send; if the server names another region, switch to it and resend.
    async fn send(&self, build: impl Fn() -> reqwest::RequestBuilder, what: &str) -> Result<reqwest::Response, String> {
        let resp = build().send().await.map_err(|e| net_error(what, e))?;
        if resp.status().is_success() {
            return Ok(resp);
        }
        let header = resp.headers().get("x-amz-bucket-region").and_then(|v| v.to_str().ok()).map(String::from);
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let current = self.region.read().expect("region lock").clone();
        match header.or_else(|| expected_region(&body)).filter(|r| *r != current) {
            Some(region) => {
                tracing::info!(from = %current, to = %region, "S3 region corrected by the server");
                *self.region.write().expect("region lock") = region;
                build().send().await.map_err(|e| net_error(what, e))
            }
            // Rebuild an error response we already consumed.
            None => Err(super::describe_failure(what, status, &body)),
        }
    }

    pub async fn test(&self) -> Result<String, String> {
        let key = self.key(".watchgrid-test");
        let body = b"watchgrid".to_vec();
        let hash = sigv4::sha256_hex(&body);
        let put = self.send(|| self.request(reqwest::Method::PUT, &key, &hash).body(body.clone()), "cannot reach the S3 endpoint").await?;
        if !put.status().is_success() {
            return Err(failure("write test failed", put).await);
        }
        let del = self.request(reqwest::Method::DELETE, &key, &sigv4::sha256_hex(b"")).send().await.map_err(|e| net_error("delete test failed", e))?;
        if !del.status().is_success() {
            return Err(failure("the test file was written but can't be deleted", del).await);
        }
        Ok(format!("Connected: can write to {}/{}", self.bucket, self.prefix).trim_end_matches('/').to_string())
    }

    pub async fn upload(&self, file: &Path, name: &str, progress: impl Fn(u64) + Send + Sync + 'static) -> Result<String, String> {
        let key = self.key(name);
        // Settle the region with a cheap request first: the body is a
        // stream and can't be resent.
        let probe_key = self.key(".watchgrid-region");
        let empty = sigv4::sha256_hex(b"");
        let _ = self.send(|| self.request(reqwest::Method::HEAD, &probe_key, &empty), "cannot reach the S3 endpoint").await;
        let (size, body) = file_body(file, progress).await?;
        let resp = self
            .request(reqwest::Method::PUT, &key, UNSIGNED_PAYLOAD)
            .header(reqwest::header::CONTENT_LENGTH, size)
            .header(reqwest::header::CONTENT_TYPE, "video/mp4")
            .body(body)
            .send()
            .await
            .map_err(|e| net_error("upload failed", e))?;
        if !resp.status().is_success() {
            return Err(failure("upload failed", resp).await);
        }
        Ok(format!("s3://{}/{key}", self.bucket))
    }
}

/// Region from the endpoint host, for the providers that put it there:
/// `s3.<region>.amazonaws.com`, `s3-<region>.amazonaws.com`,
/// `s3.<region>.backblazeb2.com`, `s3.<region>.wasabisys.com`,
/// `s3.<region>.scw.cloud`, `<region>.digitaloceanspaces.com`,
/// `<region>.your-objectstorage.com` (Hetzner), `<region>.s3.synologyc2.net`
/// (Synology C2)… Cloudflare R2 uses `auto`.
/// Anything else (MinIO on a LAN) gets `us-east-1`, which such servers accept.
fn region_of(host: &str) -> String {
    let host = host.to_ascii_lowercase();
    if host.ends_with(".r2.cloudflarestorage.com") {
        return "auto".into();
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return "us-east-1".into();
    }
    let labels: Vec<&str> = host.split('.').collect();
    // The label that names the region: after a leading "s3" label
    // (s3.<region>.…), before an inner one (<region>.s3.… — Synology C2),
    // or the first one.
    let candidate = match labels.iter().position(|l| *l == "s3") {
        Some(0) => labels.get(1).copied(),
        Some(i) => labels.get(i - 1).copied(),
        None => labels.first().and_then(|l| l.strip_prefix("s3-")).or_else(|| labels.first().copied()),
    };
    match candidate {
        // A region looks like "eu-central-1", "nyc3", "fsn1", "fr-par", "gra".
        Some(r) if labels.len() >= 3 && looks_like_region(r) => r.to_string(),
        _ => "us-east-1".into(),
    }
}

fn looks_like_region(label: &str) -> bool {
    let generic = ["www", "api", "cloud", "storage", "objects", "minio", "nas", "files", "s3", "amazonaws", "localhost"];
    !label.is_empty() && !generic.contains(&label) && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && (label.contains('-') || label.chars().any(|c| c.is_ascii_digit()) || label.len() <= 3)
}

/// "…the region 'us-east-1' is wrong; expecting 'eu-west-1'" (AuthorizationHeaderMalformed).
fn expected_region(body: &str) -> Option<String> {
    let start = body.find("expecting '")? + "expecting '".len();
    let len = body[start..].find('\'')?;
    Some(body[start..start + len].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use watchgrid_model::ExportKind;

    fn cfg<'a>(endpoint: &'a str, location: &'a str) -> TargetConfig<'a> {
        TargetConfig { kind: ExportKind::S3, endpoint, location, username: "AKID", secret: "s" }
    }

    #[test]
    fn parses_bucket_prefix_and_region() {
        let s = S3::new(&cfg("https://minio.lan:9000/", "/nvr-backup/clips/")).unwrap();
        assert_eq!((s.bucket.as_str(), s.prefix.as_str()), ("nvr-backup", "clips"));
        assert_eq!(s.key("cam/a.mp4"), "clips/cam/a.mp4");
        assert_eq!(s.region.read().unwrap().as_str(), "us-east-1");
        for (host, region) in [
            ("s3.eu-central-1.amazonaws.com", "eu-central-1"),
            ("s3-eu-west-1.amazonaws.com", "eu-west-1"),
            ("s3.amazonaws.com", "us-east-1"),
            ("s3.us-west-004.backblazeb2.com", "us-west-004"),
            ("s3.eu-central-2.wasabisys.com", "eu-central-2"),
            ("s3.fr-par.scw.cloud", "fr-par"),
            ("nyc3.digitaloceanspaces.com", "nyc3"),
            ("fsn1.your-objectstorage.com", "fsn1"),
            ("abc123.r2.cloudflarestorage.com", "auto"),
            ("eu-002.s3.synologyc2.net", "eu-002"),
            ("us-001.s3.synologyc2.net", "us-001"),
            ("minio.lan", "us-east-1"),
            ("nas.home.lan", "us-east-1"),
            ("192.168.1.112", "us-east-1"),
        ] {
            assert_eq!(region_of(host), region, "{host}");
        }
        let err = "<Error><Code>AuthorizationHeaderMalformed</Code><Message>The authorization header is malformed; the region 'us-east-1' is wrong; expecting 'eu-west-2'</Message></Error>";
        assert_eq!(expected_region(err).as_deref(), Some("eu-west-2"));
        assert!(S3::new(&cfg("ftp://x", "b")).is_err());
        assert!(S3::new(&cfg("https://x", "")).is_err());
    }
}
