//! AWS Signature Version 4 for S3 requests (header-based), as used by AWS
//! S3, MinIO, Wasabi, Backblaze B2 and other S3-compatible stores.

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// Payload hash used for streamed uploads (allowed by S3 over HTTPS and by MinIO).
pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

pub struct Credentials<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
}

/// Headers to add: `x-amz-date`, `x-amz-content-sha256`, `authorization`.
pub fn sign(creds: &Credentials<'_>, method: &str, host: &str, path: &str, query: &str, payload_hash: &str, now: DateTime<Utc>) -> Vec<(&'static str, String)> {
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = now.format("%Y%m%d").to_string();
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request = format!(
        "{method}\n{path}\n{query}\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n\n{signed_headers}\n{payload_hash}"
    );
    let scope = format!("{date}/{}/s3/aws4_request", creds.region);
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}", hex(&Sha256::digest(canonical_request.as_bytes())));
    let key = [date.as_str(), creds.region, "s3", "aws4_request"]
        .iter()
        .fold(format!("AWS4{}", creds.secret_key).into_bytes(), |key, part| hmac(&key, part.as_bytes()));
    let signature = hex(&hmac(&key, string_to_sign.as_bytes()));
    vec![
        ("x-amz-date", amz_date),
        ("x-amz-content-sha256", payload_hash.to_string()),
        ("authorization", format!("AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}", creds.access_key)),
    ]
}

/// URI-encode a path the way SigV4 expects (every byte except unreserved and `/`).
pub fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The "GET Object" example from the AWS SigV4 documentation
    /// (examplebucket, test.txt, Range: bytes=0-9 left out of our header set,
    /// so we check the derivation of the signing key and the known empty hash).
    #[test]
    fn known_values() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        // Signing key derivation from the AWS docs ("Deriving the signing key").
        let key = ["20150830", "us-east-1", "iam", "aws4_request"]
            .iter()
            .fold(b"AWS4wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_vec(), |k, p| hmac(&k, p.as_bytes()));
        assert_eq!(hex(&key), "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9");
    }

    #[test]
    fn signs_with_the_expected_shape() {
        let creds = Credentials { access_key: "AKID", secret_key: "secret", region: "us-east-1" };
        let now = "2026-09-25T12:00:00Z".parse().unwrap();
        let h = sign(&creds, "PUT", "minio.local:9000", "/bucket/a%20b.mp4", "", UNSIGNED_PAYLOAD, now);
        assert_eq!(h[0], ("x-amz-date", "20260925T120000Z".to_string()));
        assert!(h[2].1.starts_with("AWS4-HMAC-SHA256 Credential=AKID/20260925/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature="));
        assert_eq!(encode_path("/bucket/cam a/x+y.mp4"), "/bucket/cam%20a/x%2By.mp4");
    }
}
