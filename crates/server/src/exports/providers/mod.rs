//! Where clips can be uploaded. Each provider knows how to test its
//! credentials and upload one file with progress.

mod s3;
pub mod sigv4;
mod webdav;

use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use futures::Stream;
use tokio::io::AsyncReadExt;
use watchgrid_model::ExportKind;

pub enum Provider {
    S3(s3::S3),
    WebDav(webdav::WebDav),
}

/// Settings of a target, with its secret decrypted.
pub struct TargetConfig<'a> {
    pub kind: ExportKind,
    pub endpoint: &'a str,
    pub location: &'a str,
    pub username: &'a str,
    pub secret: &'a str,
}

impl Provider {
    pub fn new(c: &TargetConfig<'_>) -> Result<Self, String> {
        match c.kind {
            ExportKind::S3 => s3::S3::new(c).map(Self::S3),
            ExportKind::Nextcloud => webdav::WebDav::new(c).map(Self::WebDav),
            ExportKind::GoogleDrive | ExportKind::Dropbox => Err("Google Drive and Dropbox sign-in isn't available yet".into()),
        }
    }

    /// Write and remove a tiny test file. Returns a human message.
    pub async fn test(&self) -> Result<String, String> {
        match self {
            Self::S3(p) => p.test().await,
            Self::WebDav(p) => p.test().await,
        }
    }

    /// Upload `file` as `remote_name` (may contain `/`). `progress` gets the
    /// bytes sent so far. Returns where it ended up.
    pub async fn upload(&self, file: &Path, remote_name: &str, progress: impl Fn(u64) + Send + Sync + 'static) -> Result<String, String> {
        match self {
            Self::S3(p) => p.upload(file, remote_name, progress).await,
            Self::WebDav(p) => p.upload(file, remote_name, progress).await,
        }
    }
}

/// Shared HTTPS client: rustls with bundled roots (never the system's).
pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::Client::builder()
            .user_agent("Watchgrid")
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("HTTP client")
    })
}

/// A file as a request body stream, reporting bytes read.
async fn file_body(file: &Path, progress: impl Fn(u64) + Send + Sync + 'static) -> Result<(u64, reqwest::Body), String> {
    let f = tokio::fs::File::open(file).await.map_err(|e| format!("cannot open {}: {e}", file.display()))?;
    let size = f.metadata().await.map_err(|e| e.to_string())?.len();
    Ok((size, reqwest::Body::wrap_stream(read_chunks(f, progress))))
}

fn read_chunks(f: tokio::fs::File, progress: impl Fn(u64) + Send + Sync + 'static) -> impl Stream<Item = std::io::Result<Vec<u8>>> + Send + 'static {
    futures::stream::unfold((f, 0u64, progress), |(mut f, sent, progress)| async move {
        let mut buf = vec![0u8; 256 * 1024];
        match f.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                let sent = sent + n as u64;
                progress(sent);
                Some((Ok(buf), (f, sent, progress)))
            }
            Err(e) => Some((Err(e), (f, sent, progress))),
        }
    })
}

/// "HTTP 403: <reason>" from a failed response.
async fn failure(what: &str, resp: reqwest::Response) -> String {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let detail = extract_message(&body);
    let hint = match status.as_u16() {
        401 | 403 => " — check the credentials and permissions",
        404 => " — check the bucket / folder",
        _ => "",
    };
    if detail.is_empty() { format!("{what}: HTTP {status}{hint}") } else { format!("{what}: HTTP {status}: {detail}{hint}") }
}

/// The `<Message>` of an S3 error, or a trimmed body.
fn extract_message(body: &str) -> String {
    if let (Some(a), Some(b)) = (body.find("<Message>"), body.find("</Message>")) {
        return body[a + 9..b].to_string();
    }
    if let (Some(a), Some(b)) = (body.find("<s:message>"), body.find("</s:message>")) {
        return body[a + 11..b].to_string();
    }
    body.chars().filter(|c| !c.is_control()).take(160).collect::<String>().trim().to_string()
}

fn net_error(what: &str, e: reqwest::Error) -> String {
    let mut msg = format!("{what}: {e}");
    let mut source = std::error::Error::source(&e);
    while let Some(s) = source {
        msg.push_str(&format!(": {s}"));
        source = s.source();
    }
    msg
}

/// Live check against a real server; run with
/// `WG_EXPORT_KIND=s3 WG_EXPORT_ENDPOINT=… WG_EXPORT_LOCATION=… WG_EXPORT_USER=… WG_EXPORT_SECRET_FILE=… WG_EXPORT_FILE=… cargo test -- --ignored live_export`.
#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test]
    #[ignore = "needs a real S3/WebDAV server"]
    async fn live_export() {
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} not set"));
        let kind = if var("WG_EXPORT_KIND") == "s3" { ExportKind::S3 } else { ExportKind::Nextcloud };
        let secret = std::fs::read_to_string(var("WG_EXPORT_SECRET_FILE")).unwrap().trim().to_string();
        let (endpoint, location, user) = (var("WG_EXPORT_ENDPOINT"), var("WG_EXPORT_LOCATION"), var("WG_EXPORT_USER"));
        let p = Provider::new(&TargetConfig { kind, endpoint: &endpoint, location: &location, username: &user, secret: &secret }).unwrap();
        println!("test: {:?}", p.test().await);
        let file = std::path::PathBuf::from(var("WG_EXPORT_FILE"));
        let size = std::fs::metadata(&file).unwrap().len();
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let s2 = seen.clone();
        let started = std::time::Instant::now();
        let link = p.upload(&file, "live-test/clip.mp4", move |n| s2.store(n, std::sync::atomic::Ordering::Relaxed)).await;
        println!("upload: {link:?} — {size} bytes in {:.2} s, progress reached {}", started.elapsed().as_secs_f64(), seen.load(std::sync::atomic::Ordering::Relaxed));
        assert!(link.is_ok());
    }
}
