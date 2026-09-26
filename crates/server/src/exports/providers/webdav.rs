//! WebDAV, with Nextcloud conventions: a server URL like
//! `https://cloud.example.com` becomes
//! `https://cloud.example.com/remote.php/dav/files/<user>/`.

use std::path::Path;

use url::Url;

use super::{TargetConfig, client, failure, file_body, net_error};

pub struct WebDav {
    /// Web link to show for uploaded files (Nextcloud files app), if known.
    web: Option<String>,
    username: String,
    password: String,
    /// Folder path segments below the DAV root (for MKCOL).
    folder_parts: Vec<String>,
    root: Url,
}

impl WebDav {
    pub fn new(c: &TargetConfig<'_>) -> Result<Self, String> {
        let server = Url::parse(c.endpoint.trim()).map_err(|_| "enter the server URL, e.g. https://cloud.example.com".to_string())?;
        if !matches!(server.scheme(), "http" | "https") {
            return Err("the server URL must start with http:// or https://".into());
        }
        let user = c.username.trim();
        if user.is_empty() || c.secret.is_empty() {
            return Err("enter the username and app password".into());
        }
        // A full DAV URL is used as given; a plain server URL gets Nextcloud's.
        let path = server.path().to_ascii_lowercase();
        let nextcloud = !path.contains("remote.php") && !path.contains("dav");
        let root_str = if nextcloud {
            format!("{}/remote.php/dav/files/{user}/", server.as_str().trim_end_matches('/'))
        } else {
            format!("{}/", server.as_str().trim_end_matches('/'))
        };
        let root = Url::parse(&root_str).map_err(|e| e.to_string())?;
        let folder_parts: Vec<String> = c.location.split('/').filter(|p| !p.is_empty()).map(String::from).collect();
        let web = nextcloud.then(|| format!("{}/apps/files/?dir=/{}", server.as_str().trim_end_matches('/'), folder_parts.join("/")));
        let dav = Self { web, username: user.to_string(), password: c.secret.to_string(), folder_parts, root };
        dav.folder()?; // an unusable folder name fails here, not at upload
        Ok(dav)
    }

    /// The target folder's URL, ending in `/`.
    fn folder(&self) -> Result<Url, String> {
        let mut folder = self.root.clone();
        for p in &self.folder_parts {
            folder = folder.join(&format!("{}/", urlencode(p))).map_err(|e| e.to_string())?;
        }
        Ok(folder)
    }

    fn req(&self, method: &str, url: Url) -> reqwest::RequestBuilder {
        client().request(reqwest::Method::from_bytes(method.as_bytes()).expect("valid method"), url).basic_auth(&self.username, Some(&self.password))
    }

    /// Create `root/<parts…>` one level at a time (existing folders are fine).
    async fn ensure_folders(&self, extra: &[&str]) -> Result<Url, String> {
        let mut url = self.root.clone();
        for part in self.folder_parts.iter().map(String::as_str).chain(extra.iter().copied()) {
            url = url.join(&format!("{}/", urlencode(part))).map_err(|e| e.to_string())?;
            let resp = self.req("MKCOL", url.clone()).send().await.map_err(|e| net_error("cannot reach the server", e))?;
            // 201 created, 405 already exists.
            if !(resp.status().is_success() || resp.status().as_u16() == 405) {
                return Err(failure(&format!("cannot create folder {part}"), resp).await);
            }
        }
        Ok(url)
    }

    pub async fn test(&self) -> Result<String, String> {
        let folder = self.ensure_folders(&[]).await?;
        let file = folder.join(".watchgrid-test").map_err(|e| e.to_string())?;
        let put = self.req("PUT", file.clone()).body("watchgrid").send().await.map_err(|e| net_error("write test failed", e))?;
        if !put.status().is_success() {
            return Err(failure("write test failed", put).await);
        }
        let del = self.req("DELETE", file).send().await.map_err(|e| net_error("delete test failed", e))?;
        if !del.status().is_success() && del.status().as_u16() != 404 {
            return Err(failure("the test file was written but can't be deleted", del).await);
        }
        Ok(format!("Connected: can write to /{}", self.folder_parts.join("/")))
    }

    pub async fn upload(&self, file: &Path, name: &str, progress: impl Fn(u64) + Send + Sync + 'static) -> Result<String, String> {
        let mut parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty()).collect();
        let file_name = parts.pop().ok_or("empty file name")?;
        let folder = self.ensure_folders(&parts).await?;
        let url = folder.join(&urlencode(file_name)).map_err(|e| e.to_string())?;
        let (size, body) = file_body(file, progress).await?;
        let resp = self
            .req("PUT", url.clone())
            .header(reqwest::header::CONTENT_LENGTH, size)
            .header(reqwest::header::CONTENT_TYPE, "video/mp4")
            .body(body)
            .send()
            .await
            .map_err(|e| net_error("upload failed", e))?;
        if !resp.status().is_success() {
            return Err(failure("upload failed", resp).await);
        }
        Ok(self.web.clone().unwrap_or_else(|| url.to_string()))
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use watchgrid_model::ExportKind;

    #[test]
    fn nextcloud_urls() {
        let c = TargetConfig { kind: ExportKind::Nextcloud, endpoint: "https://cloud.example.com/", location: "/Cameras/Watch grid", username: "alice", secret: "pw" };
        let w = WebDav::new(&c).unwrap();
        assert_eq!(w.folder().unwrap().as_str(), "https://cloud.example.com/remote.php/dav/files/alice/Cameras/Watch%20grid/");
        assert_eq!(w.web.as_deref(), Some("https://cloud.example.com/apps/files/?dir=/Cameras/Watch grid"));
        let generic = TargetConfig { endpoint: "https://nas.lan/webdav", ..c };
        let g = WebDav::new(&generic).unwrap();
        assert_eq!(g.folder().unwrap().as_str(), "https://nas.lan/webdav/Cameras/Watch%20grid/");
        assert!(g.web.is_none());
    }
}
