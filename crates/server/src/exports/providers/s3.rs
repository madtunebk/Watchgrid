//! S3-compatible object storage (AWS, MinIO, Wasabi, Backblaze B2…).
//! Path-style requests, SigV4 with an unsigned streamed payload.

use std::path::Path;

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
    region: String,
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
            region: region_of(endpoint.host_str().unwrap_or("")),
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
        let path = sigv4::encode_path(&format!("{}/{}/{key}", self.endpoint.path().trim_end_matches('/'), self.bucket));
        let host = match self.endpoint.port() {
            Some(p) => format!("{}:{p}", self.endpoint.host_str().unwrap_or("")),
            None => self.endpoint.host_str().unwrap_or("").to_string(),
        };
        let url = format!("{}://{host}{path}", self.endpoint.scheme());
        let creds = Credentials { access_key: &self.access_key, secret_key: &self.secret_key, region: &self.region };
        let mut req = client().request(method.clone(), url);
        for (k, v) in sigv4::sign(&creds, method.as_str(), &host, &path, "", payload_hash, Utc::now()) {
            req = req.header(k, v);
        }
        req
    }

    pub async fn test(&self) -> Result<String, String> {
        let key = self.key(".watchgrid-test");
        let body = b"watchgrid".to_vec();
        let put = self.request(reqwest::Method::PUT, &key, &sigv4::sha256_hex(&body)).body(body).send().await.map_err(|e| net_error("cannot reach the S3 endpoint", e))?;
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

/// AWS encodes the region in the host (s3.<region>.amazonaws.com); most
/// other S3-compatible stores accept us-east-1.
fn region_of(host: &str) -> String {
    let parts: Vec<&str> = host.split('.').collect();
    match parts.iter().position(|p| p.starts_with("s3")) {
        Some(i) if host.ends_with("amazonaws.com") && parts.len() > i + 3 => parts[i + 1].to_string(),
        _ if host.contains("backblazeb2.com") => host.split('.').nth(1).unwrap_or("us-east-1").to_string(),
        _ => "us-east-1".into(),
    }
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
        assert_eq!((s.bucket.as_str(), s.prefix.as_str(), s.region.as_str()), ("nvr-backup", "clips", "us-east-1"));
        assert_eq!(s.key("cam/a.mp4"), "clips/cam/a.mp4");
        assert_eq!(region_of("s3.eu-central-1.amazonaws.com"), "eu-central-1");
        assert_eq!(region_of("s3.us-west-004.backblazeb2.com"), "us-west-004");
        assert!(S3::new(&cfg("ftp://x", "b")).is_err());
        assert!(S3::new(&cfg("https://x", "")).is_err());
    }
}
