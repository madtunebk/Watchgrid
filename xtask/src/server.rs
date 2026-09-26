//! Minimal static file server for development, with SPA fallback and
//! live reload. std only.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{fs, thread};

use crate::watch::latest_mtime;
use crate::{Opts, build, dist_dir};

/// Dev-only page showing the app at several viewport sizes at once.
const VIEWPORTS: &str = include_str!("../assets/viewports.html");

const LIVE_RELOAD: &str = r#"<script>
(() => {
  let id = null;
  setInterval(async () => {
    try {
      const r = await fetch('/__build', { cache: 'no-store' });
      const next = await r.text();
      if (id !== null && next !== id) location.reload();
      id = next;
    } catch {}
  }, 1000);
})();
</script>"#;

pub fn serve(opts: &Opts) -> Result<(), String> {
    let build_id = Arc::new(Mutex::new(build(opts).unwrap_or_else(|e| {
        eprintln!("  initial build failed: {e}");
        String::from("none")
    })));

    let addr = format!("{}:{}", opts.host, opts.port);
    let listener = TcpListener::bind(&addr).map_err(|e| format!("cannot bind {addr}: {e}"))?;
    println!("\n  Watchgrid UI → http://{addr}/   (Ctrl+C to stop)\n");

    // Rebuild on change.
    {
        let build_id = Arc::clone(&build_id);
        let opts = opts.clone();
        thread::spawn(move || {
            let mut seen = latest_mtime();
            loop {
                thread::sleep(Duration::from_millis(500));
                let now = latest_mtime();
                if now > seen {
                    // Let editors finish writing multi-file saves.
                    thread::sleep(Duration::from_millis(150));
                    seen = latest_mtime();
                    match build(&opts) {
                        Ok(id) => *build_id.lock().unwrap() = id,
                        Err(e) => eprintln!("  build failed: {e} (serving previous build)"),
                    }
                }
            }
        });
    }

    if let Some(api) = &opts.api {
        println!("  proxying /api/* → {api}\n");
    }
    let api = opts.api.clone().map(Arc::new);
    for stream in listener.incoming().flatten() {
        let build_id = Arc::clone(&build_id);
        let api = api.clone();
        thread::spawn(move || {
            let _ = handle(stream, &build_id, api.as_deref().map(String::as_str));
        });
    }
    Ok(())
}

fn handle(mut stream: TcpStream, build_id: &Mutex<String>, api: Option<&str>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut head = vec![request_line.clone()];
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? <= 2 {
            break;
        }
        head.push(line);
    }

    if let Some(base) = api
        && request_line.split_whitespace().nth(1).is_some_and(|p| p.starts_with("/api/"))
    {
        return crate::proxy::forward(&mut stream, &mut reader, &head, base);
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let path = target.split(['?', '#']).next().unwrap_or("/");

    if method != "GET" && method != "HEAD" {
        return respond(&mut stream, 405, "text/plain", b"method not allowed");
    }

    if path == "/__viewports" {
        return respond(&mut stream, 200, "text/html; charset=utf-8", VIEWPORTS.as_bytes());
    }

    if path == "/__build" {
        let id = build_id.lock().unwrap().clone();
        return respond(&mut stream, 200, "text/plain", id.as_bytes());
    }

    let dist = dist_dir();
    let file = resolve(&dist, path);
    let (file, status) = match file {
        Some(f) if f.is_file() => (f, 200),
        // Client-side routes (no extension) fall back to the app.
        _ if Path::new(path).extension().is_none() => (dist.join("index.html"), 200),
        _ => return respond(&mut stream, 404, "text/plain", b"not found"),
    };

    let Ok(mut body) = fs::read(&file) else {
        return respond(&mut stream, 503, "text/plain", b"build not ready");
    };
    if file.file_name().is_some_and(|n| n == "index.html") {
        let html = String::from_utf8_lossy(&body).replace("</body>", &format!("{LIVE_RELOAD}</body>"));
        body = html.into_bytes();
    }
    respond(&mut stream, status, content_type(&file), &body)
}

fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    let rel = url_path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    let rel = Path::new(rel);
    // Reject traversal.
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(root.join(rel))
}

fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "json" => "application/json",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn respond(stream: &mut TcpStream, status: u16, ctype: &str, body: &[u8]) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Service Unavailable",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
}
