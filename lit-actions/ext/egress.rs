//! Egress filtering primitives shared across the Lit Actions egress surface.
//!
//! A Lit Action runs untrusted JS with two independent ways to make outbound
//! HTTP requests, and *both* must be prevented from reaching internal address
//! space (loopback, RFC1918, link-local incl. cloud metadata, …) — otherwise a
//! permissioned malicious action becomes a lateral-SSRF / key-exfil primitive
//! inside the TEE pod (CPL-295):
//!
//! * `fetch()` — Deno's built-in, filtered in `lit-actions-server`'s runtime via
//!   `deny_net` + a custom fetch DNS resolver.
//! * `Lit.Actions.proxiedFetch` — a raw `reqwest::Client` op (see
//!   [`crate::bindings`]). It never touches Deno's permission engine, so it is
//!   filtered here: [`egress_filtered_reqwest_resolver`] drops disallowed
//!   resolved addresses, and [`connect_target_forbidden_ip`] rejects literal-IP
//!   connect targets that bypass DNS.
//!
//! [`is_forbidden_ip`] is the single source of truth for "internal address"
//! classification; the server's `fetch()` resolver reuses it so the two egress
//! paths can never drift.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// True if `ip` is in address space a Lit Action must not be able to reach.
///
/// Runs on the actual resolved addresses (not the URL string), so it catches
/// internal hosts named by hostname and DNS-rebinding answers alike. IPv4-in-
/// IPv6 forms are unwrapped and re-checked as IPv4 so `::ffff:127.0.0.1` cannot
/// smuggle loopback past us.
pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_forbidden_ipv4(v4),
        IpAddr::V6(v6) => is_forbidden_ipv6(v6),
    }
}

fn is_forbidden_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_loopback()            // 127.0.0.0/8
        || ip.is_private()      // 10/8, 172.16/12, 192.168/16
        || ip.is_link_local()   // 169.254.0.0/16, incl. cloud metadata
        || ip.is_unspecified()  // 0.0.0.0
        || ip.is_broadcast()    // 255.255.255.255
        || is_shared_cgnat(ip)  // 100.64.0.0/10
        || is_benchmarking(ip) // 198.18.0.0/15
}

fn is_forbidden_ipv6(ip: Ipv6Addr) -> bool {
    // Re-check any IPv4-mapped (`::ffff:a.b.c.d`) or IPv4-compatible
    // (`::a.b.c.d`) address as IPv4 so it can't smuggle a private v4 target.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_forbidden_ipv4(v4);
    }
    if let Some(v4) = ip.to_ipv4()
        && is_forbidden_ipv4(v4)
    {
        return true;
    }
    ip.is_loopback()                // ::1
        || ip.is_unspecified()      // ::
        || is_unique_local_ipv6(ip) // fc00::/7
        || is_link_local_ipv6(ip) // fe80::/10
}

/// 100.64.0.0/10 — carrier-grade NAT / shared address space (RFC 6598).
fn is_shared_cgnat(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 100 && (b & 0b1100_0000) == 0b0100_0000
}

/// 198.18.0.0/15 — network interconnect device benchmarking (RFC 2544).
fn is_benchmarking(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 198 && (b & 0b1111_1110) == 18
}

/// fc00::/7 — IPv6 unique local addresses (RFC 4193).
fn is_unique_local_ipv6(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

/// fe80::/10 — IPv6 unicast link-local addresses.
fn is_link_local_ipv6(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

/// The literal IP a `proxiedFetch` will actually *connect to*, if any.
///
/// When a proxy is set the request tunnels through the proxy, so the connect
/// target is the proxy host (the destination URL is resolved remotely, at the
/// proxy, and is not our concern). Direct (proxy-less) requests connect to the
/// destination URL's host. Returns `Some(ip)` only when that host is a literal
/// IP address — hostnames resolve through the DNS resolver, which filters them.
fn connect_target_literal_ip(url: &str, proxy: Option<&str>) -> Option<IpAddr> {
    let connect_str = proxy.unwrap_or(url);
    // A proxy string may omit the scheme (reqwest treats it as http); add one so
    // URL parsing can extract the host either way.
    let normalized;
    let to_parse = if connect_str.contains("://") {
        connect_str
    } else {
        normalized = format!("http://{connect_str}");
        &normalized
    };
    let parsed = reqwest::Url::parse(to_parse).ok()?;
    let host = parsed.host_str()?;
    // Some URL encodings bracket IPv6 hosts; strip them before parsing.
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    host.parse::<IpAddr>().ok()
}

/// True if a `proxiedFetch` for `url` (optionally via `proxy`) would connect
/// directly to a forbidden internal literal IP. This is the reqwest counterpart
/// to the `fetch()` path's `deny_net`: it closes the literal-IP hole that
/// bypasses DNS resolution.
pub fn connect_target_forbidden_ip(url: &str, proxy: Option<&str>) -> bool {
    connect_target_literal_ip(url, proxy).is_some_and(is_forbidden_ip)
}

/// A `reqwest` DNS resolver that resolves via the system resolver and strips any
/// address in forbidden internal space before returning to the connector, so a
/// hostname (or DNS-rebinding answer) pointing at internal space cannot be
/// reached. reqwest connects to exactly the addresses returned here, with no
/// second resolution, so there is no resolve-then-reconnect TOCTOU gap.
#[derive(Debug)]
struct EgressFilterResolver;

impl reqwest::dns::Resolve for EgressFilterResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_owned();
            // getaddrinfo via tokio's blocking pool. Port 0 is a placeholder —
            // reqwest substitutes the real port; we only need the addresses.
            let resolved = tokio::net::lookup_host((host.as_str(), 0)).await?;
            let allowed: Vec<SocketAddr> = resolved
                .filter(|addr| !is_forbidden_ip(addr.ip()))
                .collect();

            if allowed.is_empty() {
                let err: Box<dyn std::error::Error + Send + Sync> = format!(
                    "egress to '{host}' blocked: host resolves only to disallowed \
                     internal addresses (loopback / RFC1918 / link-local)"
                )
                .into();
                return Err(err);
            }

            let addrs: reqwest::dns::Addrs = Box::new(allowed.into_iter());
            Ok(addrs)
        })
    }
}

/// Build the egress-filtering DNS resolver to install on `proxiedFetch`'s
/// reqwest clients via `ClientBuilder::dns_resolver` (which needs a concrete
/// `Arc<R: Resolve>`, hence `impl` rather than a trait object).
pub fn egress_filtered_reqwest_resolver() -> std::sync::Arc<impl reqwest::dns::Resolve> {
    std::sync::Arc::new(EgressFilterResolver)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn forbidden(s: &str) -> bool {
        is_forbidden_ip(IpAddr::from_str(s).unwrap())
    }

    #[test]
    fn blocks_loopback_and_metadata_and_private() {
        for ip in [
            "127.0.0.1",
            "127.5.5.5",
            "169.254.169.254", // cloud metadata
            "169.254.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "100.64.0.1", // CGNAT
            "198.18.0.1", // benchmarking
            "0.0.0.0",
            "255.255.255.255",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "::ffff:127.0.0.1", // IPv4-mapped loopback
            "::ffff:169.254.169.254",
        ] {
            assert!(forbidden(ip), "{ip} should be forbidden");
        }
    }

    #[test]
    fn allows_public_addresses() {
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.216.34",        // example.com
            "172.32.0.1",           // just outside 172.16/12
            "100.128.0.1",          // just outside CGNAT 100.64/10
            "198.20.0.1",           // just outside benchmarking
            "2606:4700:4700::1111", // cloudflare v6
        ] {
            assert!(!forbidden(ip), "{ip} should be allowed");
        }
    }

    #[test]
    fn direct_literal_ip_target_is_evaluated() {
        // No proxy: the destination URL host is the connect target.
        assert!(connect_target_forbidden_ip(
            "http://127.0.0.1:5001/api/v0/pin/add",
            None
        ));
        assert!(connect_target_forbidden_ip(
            "http://169.254.169.254/latest/",
            None
        ));
        assert!(connect_target_forbidden_ip("http://[::1]:8080/", None));
        assert!(!connect_target_forbidden_ip("https://example.com/", None));
        assert!(!connect_target_forbidden_ip("https://8.8.8.8/", None));
        // Hostnames are handled by the resolver, not this literal-IP guard.
        assert!(!connect_target_forbidden_ip("http://localhost:5001/", None));
    }

    #[test]
    fn proxy_host_is_the_connect_target_when_proxied() {
        // A forbidden proxy is blocked regardless of the (remote-resolved) URL.
        assert!(connect_target_forbidden_ip(
            "https://api.binance.com/",
            Some("http://127.0.0.1:8888")
        ));
        assert!(connect_target_forbidden_ip(
            "https://api.binance.com/",
            Some("127.0.0.1:8888") // scheme-less proxy
        ));
        assert!(connect_target_forbidden_ip(
            "https://api.binance.com/",
            Some("http://user:pass@10.0.0.5:3128")
        ));
        // A public proxy is fine even if the destination *looks* internal: that
        // destination is resolved at the proxy, not from inside the enclave.
        assert!(!connect_target_forbidden_ip(
            "http://127.0.0.1/",
            Some("http://proxy.example.com:3128")
        ));
    }
}
