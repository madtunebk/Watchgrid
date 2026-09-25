//! The client's address, for the sign-in rate limit and the sessions list.
//!
//! Behind a reverse proxy (nginx for HTTPS) every request comes from the
//! proxy. When the connecting address is listed in
//! `WATCHGRID_TRUSTED_PROXIES` (IPs or CIDR ranges, comma-separated), the
//! client is taken from `X-Forwarded-For` (or `X-Real-IP`). From anyone
//! else those headers are ignored: they are trivial to fake.

use std::net::IpAddr;
use std::sync::OnceLock;

use axum::http::HeaderMap;

/// An address range: `192.168.1.10`, `10.0.0.0/8`, `::1`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Range {
    net: IpAddr,
    prefix: u8,
}

impl Range {
    fn parse(s: &str) -> Option<Self> {
        let (ip, prefix) = match s.split_once('/') {
            Some((ip, p)) => (ip.trim().parse::<IpAddr>().ok()?, Some(p.trim().parse::<u8>().ok()?)),
            None => (s.trim().parse::<IpAddr>().ok()?, None),
        };
        let max = if ip.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(max);
        (prefix <= max).then_some(Self { net: ip, prefix })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        match (self.net, ip) {
            (IpAddr::V4(n), IpAddr::V4(a)) => same_prefix(u128::from(u32::from(n)), u128::from(u32::from(a)), self.prefix, 32),
            (IpAddr::V6(n), IpAddr::V6(a)) => same_prefix(u128::from(n), u128::from(a), self.prefix, 128),
            _ => false,
        }
    }
}

fn same_prefix(a: u128, b: u128, prefix: u8, bits: u32) -> bool {
    let shift = bits - u32::from(prefix);
    shift >= bits || (a >> shift) == (b >> shift)
}

fn parse_list(s: &str) -> Vec<Range> {
    s.split(',').map(str::trim).filter(|p| !p.is_empty()).filter_map(|p| {
        let r = Range::parse(p);
        if r.is_none() {
            tracing::warn!("WATCHGRID_TRUSTED_PROXIES: `{p}` is not an IP address or range; ignored");
        }
        r
    }).collect()
}

fn trusted() -> &'static [Range] {
    static TRUSTED: OnceLock<Vec<Range>> = OnceLock::new();
    TRUSTED.get_or_init(|| std::env::var("WATCHGRID_TRUSTED_PROXIES").map(|v| parse_list(&v)).unwrap_or_default())
}

/// The client behind `peer` (the connecting address).
pub fn client_ip(peer: IpAddr, headers: &HeaderMap) -> IpAddr {
    resolve(peer, headers, trusted())
}

fn resolve(peer: IpAddr, headers: &HeaderMap, trusted: &[Range]) -> IpAddr {
    let is_trusted = |ip: IpAddr| trusted.iter().any(|r| r.contains(ip));
    if !is_trusted(peer) {
        return peer;
    }
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    // Each proxy appends the address it received from: walk back from the
    // nearest hop and stop at the first one that isn't a trusted proxy.
    if let Some(chain) = header("x-forwarded-for") {
        let hops: Vec<IpAddr> = chain.split(',').filter_map(|h| h.trim().parse().ok()).collect();
        return hops.iter().rev().copied().find(|ip| !is_trusted(*ip)).or(hops.first().copied()).unwrap_or(peer);
    }
    header("x-real-ip").and_then(|v| v.trim().parse().ok()).unwrap_or(peer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, v.parse().unwrap());
        }
        h
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn ranges_parse_and_match() {
        let r = parse_list("127.0.0.1, 192.168.1.0/24, ::1, nonsense, 10.0.0.0/33");
        assert_eq!(r.len(), 3, "bad entries are skipped");
        assert!(r[0].contains(ip("127.0.0.1")) && !r[0].contains(ip("127.0.0.2")));
        assert!(r[1].contains(ip("192.168.1.159")) && !r[1].contains(ip("192.168.2.1")));
        assert!(r[2].contains(ip("::1")) && !r[2].contains(ip("127.0.0.1")));
        assert!(Range::parse("0.0.0.0/0").unwrap().contains(ip("8.8.8.8")));
    }

    #[test]
    fn forwarded_headers_count_only_from_trusted_proxies() {
        let trusted = parse_list("127.0.0.1");
        let h = headers(&[("x-forwarded-for", "192.168.1.50")]);
        assert_eq!(resolve(ip("127.0.0.1"), &h, &trusted), ip("192.168.1.50"), "via nginx");
        assert_eq!(resolve(ip("192.168.1.66"), &h, &trusted), ip("192.168.1.66"), "direct: header ignored");
        assert_eq!(resolve(ip("127.0.0.1"), &h, &[]), ip("127.0.0.1"), "nothing trusted by default");
    }

    #[test]
    fn a_faked_hop_in_front_does_not_win() {
        // The client sent "1.2.3.4" itself; nginx appended the real address.
        let trusted = parse_list("127.0.0.1");
        let h = headers(&[("x-forwarded-for", "1.2.3.4, 192.168.1.50")]);
        assert_eq!(resolve(ip("127.0.0.1"), &h, &trusted), ip("192.168.1.50"));
        let real = headers(&[("x-real-ip", "192.168.1.51")]);
        assert_eq!(resolve(ip("127.0.0.1"), &real, &trusted), ip("192.168.1.51"));
    }
}
