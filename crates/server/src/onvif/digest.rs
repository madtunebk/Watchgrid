//! HTTP Digest login (RFC 7616: MD5 or SHA-256, `qop=auth` or none), for
//! cameras that want it besides, or instead of, WS-Security. Answering a
//! challenge never sends the password itself.

use md5::Md5;
use sha2::{Digest as _, Sha256};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Algorithm {
    Md5,
    Sha256,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Challenge {
    realm: String,
    nonce: String,
    opaque: Option<String>,
    algorithm: Algorithm,
    /// The camera offers `qop=auth` (then nc/cnonce are part of the answer).
    qop_auth: bool,
}

/// The scheme of a `WWW-Authenticate` value ("digest", "basic", …).
pub fn scheme(header: &str) -> String {
    header.split_whitespace().next().unwrap_or("").to_ascii_lowercase()
}

/// A Digest challenge we can answer; `None` for other schemes and for
/// algorithms we don't support (`-sess`, SHA-512-256).
pub fn parse(header: &str) -> Option<Challenge> {
    let rest = header.trim_start();
    if scheme(rest) != "digest" {
        return None;
    }
    let params = params(&rest[6..]);
    let get = |k: &str| params.iter().find(|(key, _)| key.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone());
    let algorithm = match get("algorithm").as_deref().map(str::to_ascii_uppercase).as_deref() {
        None | Some("MD5") => Algorithm::Md5,
        Some("SHA-256") => Algorithm::Sha256,
        Some(_) => return None,
    };
    let qop = get("qop").unwrap_or_default();
    Some(Challenge {
        realm: get("realm")?,
        nonce: get("nonce")?,
        opaque: get("opaque"),
        algorithm,
        qop_auth: qop.split(',').any(|q| q.trim().eq_ignore_ascii_case("auth")),
    })
}

/// `key=value, key="quoted, value"` pairs.
fn params(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        let Some(eq) = rest.find('=') else { break };
        let key = rest[..eq].trim().trim_start_matches(',').trim().to_string();
        rest = rest[eq + 1..].trim_start();
        let value;
        if let Some(quoted) = rest.strip_prefix('"') {
            let mut v = String::new();
            let mut chars = quoted.char_indices();
            let mut end = quoted.len();
            while let Some((i, c)) = chars.next() {
                match c {
                    '\\' => v.extend(chars.next().map(|(_, c)| c)),
                    '"' => {
                        end = i + 1;
                        break;
                    }
                    c => v.push(c),
                }
            }
            value = v;
            rest = &quoted[end.min(quoted.len())..];
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            value = rest[..end].trim().to_string();
            rest = &rest[end..];
        }
        out.push((key, value));
        rest = rest.trim_start().trim_start_matches(',').trim_start();
    }
    out
}

impl Challenge {
    fn hash(&self, s: &str) -> String {
        match self.algorithm {
            Algorithm::Md5 => hex(&Md5::digest(s.as_bytes())),
            Algorithm::Sha256 => hex(&Sha256::digest(s.as_bytes())),
        }
    }

    /// The `Authorization` header value for request `nc` (1, 2, … per nonce).
    pub fn authorization(&self, username: &str, password: &str, method: &str, uri: &str, nc: u32, cnonce: &str) -> String {
        let ha1 = self.hash(&format!("{username}:{}:{password}", self.realm));
        let ha2 = self.hash(&format!("{method}:{uri}"));
        let nc = format!("{nc:08x}");
        let response = if self.qop_auth {
            self.hash(&format!("{ha1}:{}:{nc}:{cnonce}:auth:{ha2}", self.nonce))
        } else {
            self.hash(&format!("{ha1}:{}:{ha2}", self.nonce))
        };
        let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let mut h = format!(r#"Digest username="{}", realm="{}", nonce="{}", uri="{}", response="{response}""#, q(username), q(&self.realm), q(&self.nonce), q(uri));
        if self.algorithm == Algorithm::Sha256 {
            h.push_str(", algorithm=SHA-256");
        }
        if self.qop_auth {
            h.push_str(&format!(r#", qop=auth, nc={nc}, cnonce="{cnonce}""#));
        }
        if let Some(opaque) = &self.opaque {
            h.push_str(&format!(r#", opaque="{}""#, q(opaque)));
        }
        h
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_the_rfc_2617_example() {
        // RFC 2617 §3.5 (Mufasa / Circle Of Life), qop=auth, MD5.
        let c = parse(r#"Digest realm="testrealm@host.com", qop="auth,auth-int", nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093", opaque="5ccc069c403ebaf9f0171e9517f40e41""#).unwrap();
        let h = c.authorization("Mufasa", "Circle Of Life", "GET", "/dir/index.html", 1, "0a4f113b");
        assert!(h.contains(r#"response="6629fae49393a05397450978507c4ef1""#), "{h}");
        assert!(h.contains("qop=auth, nc=00000001") && h.contains(r#"opaque="5ccc069c403ebaf9f0171e9517f40e41""#), "{h}");
    }

    #[test]
    fn answers_the_rfc_7616_sha_256_example() {
        let c = parse(r#"Digest realm="http-auth@example.org", qop="auth, auth-int", algorithm=SHA-256, nonce="7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v", opaque="FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS""#).unwrap();
        let h = c.authorization("Mufasa", "Circle of Life", "GET", "/dir/index.html", 1, "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ");
        assert!(h.contains(r#"response="753927fa0e85d155564e2e272a28d1802ca10daf4496794697cf8db5856cb6c1""#), "{h}");
        assert!(h.contains("algorithm=SHA-256"), "{h}");
    }

    #[test]
    fn without_qop_the_legacy_answer() {
        let c = parse(r#"Digest realm="cam", nonce="abc""#).unwrap();
        let h = c.authorization("admin", "pw", "POST", "/onvif/device_service", 1, "x");
        assert!(!h.contains("qop") && !h.contains("nc="), "{h}");
    }

    #[test]
    fn only_digest_challenges_we_can_answer() {
        assert_eq!(parse(r#"Basic realm="cam""#), None);
        assert_eq!(parse(r#"Digest realm="cam", nonce="n", algorithm=MD5-sess"#), None);
        assert_eq!(parse(r#"Digest realm="cam""#), None, "no nonce");
        assert_eq!(scheme(r#"Basic realm="x""#), "basic");
        let quoted = parse(r#"Digest realm="a, \"b\"", nonce="n""#).unwrap();
        assert_eq!(quoted.realm, r#"a, "b""#);
    }
}
