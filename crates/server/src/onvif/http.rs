//! Minimal HTTP/1.1 POST for SOAP: one request per connection. Plain
//! `http://` only (ONVIF devices on a LAN); no redirects.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use url::Url;

const TIMEOUT: Duration = Duration::from_secs(8);
const MAX_BODY: usize = 4 << 20;

pub struct Reply {
    pub status: u16,
    pub body: String,
}

pub async fn post_soap(url: &Url, action: &str, body: &str) -> Result<Reply, String> {
    if url.scheme() != "http" {
        return Err("only http:// ONVIF addresses are supported".into());
    }
    let host = url.host_str().ok_or("the ONVIF URL has no host")?;
    let port = url.port().unwrap_or(80);
    let path = match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().to_string(),
    };
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/soap+xml; charset=utf-8; action=\"{action}\"\r\nContent-Length: {}\r\nConnection: close\r\nUser-Agent: Watchgrid\r\n\r\n{body}",
        body.len()
    );
    let exchange = async {
        let mut stream = TcpStream::connect((host, port)).await.map_err(|e| {
            // Many cameras serve ONVIF on a non-standard port.
            let hint = if url.port().is_none() { " — ONVIF often uses another port, e.g. :2020 (TP-Link), :8000, :8080 or :8899" } else { "" };
            format!("cannot connect to {host}:{port}: {e}{hint}")
        })?;
        stream.write_all(request.as_bytes()).await.map_err(|e| e.to_string())?;
        // Read until the response is complete; some devices keep the
        // connection open despite `Connection: close`.
        let mut raw = Vec::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&buf[..n]);
            if raw.len() > MAX_BODY {
                return Err("the ONVIF reply is too large".into());
            }
            if complete(&raw) {
                break;
            }
        }
        parse_response(&raw)
    };
    timeout(TIMEOUT, exchange).await.map_err(|_| format!("{host}:{port} did not answer in {} s", TIMEOUT.as_secs()))?
}

/// Has a full response (per Content-Length or the chunked terminator) arrived?
fn complete(raw: &[u8]) -> bool {
    let Some(split) = raw.windows(4).position(|w| w == b"\r\n\r\n") else { return false };
    let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
    let body = &raw[split + 4..];
    if let Some(len) = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()) {
        return body.len() >= len;
    }
    if head.contains("transfer-encoding:") && head.contains("chunked") {
        return body.ends_with(b"0\r\n\r\n");
    }
    false
}

fn parse_response(raw: &[u8]) -> Result<Reply, String> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| if raw.is_empty() { "the camera closed the connection without answering".to_string() } else { "malformed HTTP response".to_string() })?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let mut body = raw[split + 4..].to_vec();
    let status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).ok_or("malformed HTTP status")?;
    if head.lines().any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.to_ascii_lowercase().contains("chunked")) {
        body = dechunk(&body)?;
    }
    Ok(Reply { status, body: String::from_utf8_lossy(&body).into_owned() })
}

fn dechunk(mut data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line_end = data.windows(2).position(|w| w == b"\r\n").ok_or("bad chunked body")?;
        let size_hex = String::from_utf8_lossy(&data[..line_end]);
        let size = usize::from_str_radix(size_hex.split(';').next().unwrap_or("").trim(), 16).map_err(|_| "bad chunk size")?;
        data = &data[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if data.len() < size {
            return Err("truncated chunk".into());
        }
        out.extend_from_slice(&data[..size]);
        data = data.get(size + 2..).unwrap_or(&[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_when_a_reply_is_complete() {
        assert!(!complete(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhel"));
        assert!(complete(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello"));
        assert!(complete(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\na\r\n0\r\n\r\n"));
        assert!(!complete(b"HTTP/1.1 200 OK\r\n"));
    }

    #[test]
    fn parses_plain_and_chunked_replies() {
        let r = parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello").unwrap();
        assert_eq!((r.status, r.body.as_str()), (200, "hello"));
        let r = parse_response(b"HTTP/1.1 400 Bad\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n").unwrap();
        assert_eq!((r.status, r.body.as_str()), (400, "abcde"));
        assert!(parse_response(b"garbage").is_err());
    }
}
