//! Forward `/api/*` requests from the dev server to the Watchgrid server,
//! so the UI can run against the real API with live reload. Plain HTTP/1.1,
//! one request per connection, WebSocket upgrades passed through; dev-only.

use std::io::{self, BufRead, Write};
use std::net::TcpStream;

/// `http://host:port` → `host:port`
fn authority(base: &str) -> &str {
    base.trim_start_matches("http://").split('/').next().unwrap_or("127.0.0.1:8090")
}

/// `head` is the request line plus header lines (each ending in CRLF);
/// the body, if any, is still unread in `reader`.
pub fn forward(client: &mut TcpStream, reader: &mut impl BufRead, head: &[String], base: &str) -> io::Result<()> {
    let length = head
        .iter()
        .find_map(|h| {
            let (k, v) = h.split_once(':')?;
            k.eq_ignore_ascii_case("content-length").then(|| v.trim().parse::<usize>().ok()).flatten()
        })
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    let target = authority(base);
    let mut upstream = match TcpStream::connect(target) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!(r#"{{"status":502,"code":"bad_gateway","message":"Watchgrid server at {target} is not reachable: {e}"}}"#);
            return write!(client, "HTTP/1.1 502 Bad Gateway\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{msg}", msg.len());
        }
    };

    // WebSocket handshakes must keep `Connection: Upgrade`; everything
    // else is sent as one request per connection.
    let upgrade = head.iter().any(|h| h.to_ascii_lowercase().starts_with("upgrade:"));
    let mut out = String::new();
    for (i, line) in head.iter().enumerate() {
        let name = line.split(':').next().unwrap_or("").to_ascii_lowercase();
        if i > 0 && (name == "host" || (name == "connection" && !upgrade)) {
            continue;
        }
        out.push_str(line.trim_end());
        out.push_str("\r\n");
    }
    out.push_str(&format!("Host: {target}\r\n"));
    if !upgrade {
        out.push_str("Connection: close\r\n");
    }
    out.push_str("\r\n");
    upstream.write_all(out.as_bytes())?;
    upstream.write_all(&body)?;

    // Pump both directions until either side closes.
    let mut to_upstream = upstream.try_clone()?;
    let mut from_client = client.try_clone()?;
    let pump = std::thread::spawn(move || {
        let _ = io::copy(&mut from_client, &mut to_upstream);
        let _ = to_upstream.shutdown(std::net::Shutdown::Write);
    });
    let _ = io::copy(&mut upstream, client);
    let _ = client.shutdown(std::net::Shutdown::Both);
    let _ = pump.join();
    Ok(())
}
