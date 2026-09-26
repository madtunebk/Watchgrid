//! Credentials embedded in stream URLs (`rtsp://user:pass@host/…`) must
//! never be stored or returned in plaintext. They are moved into the
//! camera's username/password (encrypted) and the URL is stored clean.

/// A URL split into its credential-free form and the `user:password` it carried.
#[derive(Debug, PartialEq, Eq)]
pub struct Split {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Removes `userinfo@` from `scheme://userinfo@host…`. Percent-encoded
/// characters in the credentials are decoded.
pub fn split(url: &str) -> Split {
    let url = url.trim();
    let clean = |u: &str| Split { url: u.to_string(), username: None, password: None };
    let Some((scheme, rest)) = url.split_once("://") else { return clean(url) };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(authority_end);
    let Some((userinfo, host)) = authority.rsplit_once('@') else { return clean(url) };
    let (user, pass) = match userinfo.split_once(':') {
        Some((u, p)) => (u, Some(p)),
        None => (userinfo, None),
    };
    Split {
        url: format!("{scheme}://{host}{path}"),
        username: Some(decode(user)).filter(|u| !u.is_empty()),
        password: pass.map(decode).filter(|p| !p.is_empty()),
    }
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_urls_are_untouched() {
        let s = split("rtsp://192.168.1.26:554/stream1");
        assert_eq!(s, Split { url: "rtsp://192.168.1.26:554/stream1".into(), username: None, password: None });
    }

    #[test]
    fn credentials_are_extracted() {
        let s = split("rtsp://admin:s3cret@192.168.1.26:554/Streaming/Channels/101");
        assert_eq!(s.url, "rtsp://192.168.1.26:554/Streaming/Channels/101");
        assert_eq!(s.username.as_deref(), Some("admin"));
        assert_eq!(s.password.as_deref(), Some("s3cret"));
    }

    #[test]
    fn percent_encoding_and_at_signs_in_passwords() {
        let s = split("rtsp://admin:p%40ss%3Aword@cam.local/live");
        assert_eq!(s.url, "rtsp://cam.local/live");
        assert_eq!(s.password.as_deref(), Some("p@ss:word"));
        // An unencoded '@' in the password: the last '@' before the path ends the userinfo.
        let s = split("rtsp://admin:a@b@cam.local/live");
        assert_eq!(s.password.as_deref(), Some("a@b"));
        assert_eq!(s.url, "rtsp://cam.local/live");
    }

    #[test]
    fn user_without_password() {
        let s = split("rtsp://viewer@cam.local/live");
        assert_eq!(s.username.as_deref(), Some("viewer"));
        assert_eq!(s.password, None);
        assert_eq!(s.url, "rtsp://cam.local/live");
    }

    #[test]
    fn at_sign_in_path_is_not_userinfo() {
        assert_eq!(split("rtsp://cam.local/live@1").url, "rtsp://cam.local/live@1");
    }
}
