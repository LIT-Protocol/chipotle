//! Egress filtering for the built-in `fetch()` path of user-authored Lit Actions.
//!
//! A Lit Action runs untrusted JS inside the sandbox with outbound `fetch()`
//! enabled (see `BASE_PERMISSIONS` in `runtime.rs`). Without a filter, an action
//! can reach anything the TEE pod can reach: the co-located kubo IPFS daemon on
//! `127.0.0.1:5001`, internal control-plane services by hostname, and the cloud
//! metadata endpoint on `169.254.169.254`. That is a lateral SSRF / key-exfil
//! primitive (CPL-295), so we block traffic to internal address space.
//!
//! Two independent layers are needed because they cover disjoint cases:
//!
//! 1. [`DENY_NET`] is handed to Deno's permission engine as `deny_net`. Deno
//!    checks the *URL host* before connecting, so this blocks literal-IP URLs
//!    such as `http://127.0.0.1:5001` or `http://169.254.169.254/…`. hyper's
//!    connector short-circuits DNS for literal IPs, so the custom resolver
//!    below never sees them — the permission layer is the only thing that can.
//!
//! 2. [`egress_filtered_resolver`] is a custom `fetch` DNS resolver. Deno's
//!    permission check only inspects the *hostname string*, so a URL like
//!    `http://litos-host:8080` or a DNS-rebinding record that resolves to an
//!    RFC1918/loopback address sails past `deny_net`. This resolver performs
//!    the lookup, drops every disallowed address, and hands hyper only the
//!    survivors — hyper connects to exactly those, with no second resolution,
//!    so there is no resolve-then-reconnect TOCTOU gap.
//!
//! The `proxiedFetch` op has its own (reqwest-based) egress surface; both paths
//! classify "internal" via [`lit_actions_ext::egress::is_forbidden_ip`] so they
//! can never drift.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use deno_runtime::deno_fetch::dns::{Resolve, Resolver, Resolving};
use deno_runtime::deno_permissions::{Host, NetDescriptor, Permissions, UnaryPermission};
use hyper_util::client::legacy::connect::dns::Name;
use ipnetwork::IpNetwork;
use lit_actions_ext::egress::{egress_allowlist, is_forbidden_ip};

/// Address blocks that user `fetch()` must never reach, expressed as Deno net
/// descriptors. Fed into `PermissionsOptions::deny_net`; deny always wins over
/// the "allow all" grant, so these carve holes out of the otherwise-open net
/// permission.
///
/// This layer exists specifically to cover *literal-IP* URLs (e.g.
/// `http://127.0.0.1:5001`): hyper's connector resolves those directly and never
/// consults [`egress_filtered_resolver`], so the permission engine is the only
/// thing that can stop them. Deno checks the URL host against this list before
/// connecting.
///
/// Only IPv4 CIDRs and *single* bracketed IPv6 literals are listed here: Deno's
/// net-descriptor *string* parser (v0.107) does not accept IPv6 CIDR subnets —
/// it splits host from port on `:`, which mangles any bare/subnet IPv6 form. The
/// IPv6 *ranges* (unique-local `fc00::/7`, link-local `fe80::/10`) are instead
/// added as `Host::IpSubnet` deny descriptors constructed directly in
/// [`base_net_permission`] — see [`DENY_NET_IPV6_SUBNETS`]. That closes the
/// literal IPv6-range `fetch()` hole (e.g. `http://[fd00:ec2::254]/`, the AWS
/// IMDSv6 metadata address) that would otherwise bypass both this string list
/// and the resolver's [`is_forbidden_ip`] (literal IPs never hit the resolver).
pub(crate) const DENY_NET: &[&str] = &[
    "0.0.0.0/8",      // "this host on this network" (RFC 1122)
    "10.0.0.0/8",     // RFC1918 private
    "100.64.0.0/10",  // CGNAT / shared address space (RFC 6598)
    "127.0.0.0/8",    // IPv4 loopback
    "169.254.0.0/16", // link-local, incl. cloud metadata 169.254.169.254
    "172.16.0.0/12",  // RFC1918 private
    "192.168.0.0/16", // RFC1918 private
    "198.18.0.0/15",  // benchmarking (RFC 2544)
    "[::]",           // IPv6 unspecified
    "[::1]",          // IPv6 loopback
];

/// The `deny_net` list actually handed to Deno's permission engine: [`DENY_NET`]
/// minus any entry whose range contains an operator-allowlisted IP
/// ([`lit_actions_ext::egress::EGRESS_ALLOWLIST_ENV`]). In production the
/// allowlist is empty, so this returns the full `DENY_NET`.
///
/// This is required because `deny_net` gates literal-IP `fetch()` URLs at the
/// permission layer *before* the DNS resolver runs (hyper never resolves a
/// literal IP), so an allowlisted literal like `127.0.0.1` must be carved out
/// here — the resolver-level `is_forbidden_ip` allowlist alone cannot reach it.
pub(crate) fn effective_deny_net() -> Vec<String> {
    let allow = egress_allowlist();
    DENY_NET
        .iter()
        .filter(|entry| !allow.iter().any(|ip| cidr_contains(entry, *ip)))
        .map(|s| s.to_string())
        .collect()
}

/// IPv6 ranges a user `fetch()` must never reach, as CIDR strings.
///
/// These cannot go in [`DENY_NET`]: Deno's `deny_net` *string* parser splits the
/// descriptor on `:` to peel off a port, which mangles any IPv6 subnet form, so
/// there is no string that yields an IPv6 `Host::IpSubnet` deny rule. Instead
/// [`base_net_permission`] parses these into [`IpNetwork`] and constructs the
/// deny descriptors directly. Kept in lockstep with the IPv6 ranges enforced on
/// the resolver path by `is_forbidden_ipv6` in `lit-actions-ext` (F-01).
pub(crate) const DENY_NET_IPV6_SUBNETS: &[&str] = &[
    "fc00::/7",  // unique-local (ULA), incl. fd00:ec2::254 (AWS IMDSv6 metadata)
    "fe80::/10", // link-local
];

/// Build the `net` permission for user `fetch()`: allow all outbound hosts, but
/// deny every internal range.
///
/// Deno's `deny_net` string option ([`effective_deny_net`]) can express the IPv4
/// CIDRs and single IPv6 literals, but *not* IPv6 CIDR subnets (its parser trips
/// over the `:`). A literal IPv6-range URL such as `http://[fd00:ec2::254]/`
/// (AWS IMDSv6) would therefore sail past the permission layer, and — being a
/// literal IP — never reach the egress DNS resolver either. This closes that gap
/// by adding [`DENY_NET_IPV6_SUBNETS`] as `Host::IpSubnet` deny descriptors,
/// which the string parser cannot produce (F-01).
///
/// The operator allowlist ([`egress_allowlist`]) carves matching entries out of
/// both layers, exactly as [`effective_deny_net`] does for the IPv4 side.
///
/// Blast radius of the carve: Deno's permission model is deny-always-wins with no
/// per-IP allow override, so exempting a literal internal IP means dropping the
/// *entire* covering deny descriptor. For the IPv6 ranges that descriptor is a
/// very large subnet (`fc00::/7`, `fe80::/10`), so allowlisting a single ULA or
/// link-local address re-opens literal-IP `fetch()` to that whole range. This is
/// an operator-only, prod-empty escape hatch (see [`EGRESS_ALLOWLIST_ENV`]), so
/// the coarse carve is accepted rather than worked around with subnet splitting —
/// but each dropped range is logged at WARN so the widening is visible.
pub(crate) fn base_net_permission() -> UnaryPermission<NetDescriptor> {
    // IPv4 CIDRs + single IPv6 literals, via the string parser. These all parse
    // (no bare IPv6 subnet among them); a bad entry is a build-time bug.
    let mut deny: Vec<NetDescriptor> = effective_deny_net()
        .iter()
        .map(|entry| {
            NetDescriptor::parse_for_list(entry)
                .expect("DENY_NET entries must be valid net descriptors")
        })
        .collect();

    // IPv6 CIDR subnets, constructed directly (the string parser can't).
    let allow = egress_allowlist();
    for cidr in DENY_NET_IPV6_SUBNETS {
        let net: IpNetwork = cidr
            .parse()
            .expect("DENY_NET_IPV6_SUBNETS entries must be valid CIDRs");
        // Match effective_deny_net: drop a range that covers an allowlisted IP.
        // Deny-wins semantics mean there's no finer carve than the whole subnet,
        // so make the (large) widening loud rather than silent.
        if let Some(ip) = allow.iter().find(|ip| net.contains(**ip)) {
            tracing::warn!(
                allowlisted_ip = %ip,
                subnet = %cidr,
                "egress allowlist entry falls inside an internal IPv6 range; \
                 dropping the entire deny subnet re-opens literal-IP fetch() to \
                 the whole range"
            );
            continue;
        }
        deny.push(NetDescriptor(Host::IpSubnet(net), None));
    }

    // allow_list = Some(empty) => allow all outbound hosts; deny_list carves the
    // internal ranges back out. deny always wins. No prompting (headless).
    Permissions::new_unary(Some(vec![]), Some(deny), false)
}

/// True if the Deno net-descriptor `entry` (an IPv4 CIDR like `10.0.0.0/8`, or a
/// bracketed single IPv6 literal like `[::1]`) contains `ip`.
fn cidr_contains(entry: &str, ip: IpAddr) -> bool {
    let entry = entry
        .strip_prefix('[')
        .and_then(|e| e.strip_suffix(']'))
        .unwrap_or(entry);
    match entry.split_once('/') {
        None => entry.parse::<IpAddr>().is_ok_and(|base| base == ip),
        Some((base, prefix)) => {
            let Ok(prefix) = prefix.parse::<u32>() else {
                return false;
            };
            match (base.parse::<IpAddr>(), ip) {
                (Ok(IpAddr::V4(base)), IpAddr::V4(ip)) if prefix <= 32 => {
                    let mask = if prefix == 0 {
                        0
                    } else {
                        u32::MAX << (32 - prefix)
                    };
                    (u32::from(base) & mask) == (u32::from(ip) & mask)
                }
                (Ok(IpAddr::V6(base)), IpAddr::V6(ip)) if prefix <= 128 => {
                    let mask = if prefix == 0 {
                        0
                    } else {
                        u128::MAX << (128 - prefix)
                    };
                    (u128::from(base) & mask) == (u128::from(ip) & mask)
                }
                _ => false,
            }
        }
    }
}

/// A `fetch` DNS resolver that resolves via the system resolver and strips any
/// address in forbidden internal space before returning to hyper.
#[derive(Debug)]
struct EgressFilterResolver;

impl Resolve for EgressFilterResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let host = name.as_str().to_owned();
            // Resolve via getaddrinfo (tokio's blocking threadpool). The port
            // is a placeholder — hyper substitutes the real one when connecting;
            // we only care about the addresses.
            let resolved = tokio::net::lookup_host((host.as_str(), 0)).await?;
            let allowed: Vec<SocketAddr> = resolved
                .filter(|addr| !is_forbidden_ip(addr.ip()))
                .collect();

            if allowed.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "fetch() egress to '{host}' blocked: host resolves only to \
                         disallowed internal addresses (loopback / RFC1918 / link-local)"
                    ),
                ));
            }

            Ok(allowed.into_iter())
        })
    }
}

/// Build the egress-filtering DNS resolver used for user `fetch()`.
pub(crate) fn egress_filtered_resolver() -> Resolver {
    Resolver::Custom(Arc::new(EgressFilterResolver))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deno_runtime::deno_permissions::{Permissions, PermissionsOptions};

    fn base_permissions() -> deno_runtime::deno_permissions::PermissionsContainer {
        let parser = deno_runtime::permissions::RuntimePermissionDescriptorParser::new(
            sys_traits::impls::RealSys,
        );
        let mut perms = Permissions::from_options(
            &parser,
            &PermissionsOptions {
                allow_net: Some(vec![]),
                deny_net: Some(DENY_NET.iter().map(|s| s.to_string()).collect()),
                ..Default::default()
            },
        )
        // Panics here mean an entry in DENY_NET is not a valid net descriptor;
        // in production this same call in BASE_PERMISSIONS would panic at first
        // use.
        .expect("DENY_NET entries must be valid net descriptors");
        // Mirror production (runtime.rs BASE_PERMISSIONS): override `net` so the
        // IPv6 CIDR subnets that the string parser can't express are enforced.
        perms.net = base_net_permission();
        deno_runtime::deno_permissions::PermissionsContainer::new(Arc::new(parser), perms)
    }

    /// The permission layer must deny literal-IP URLs pointing at internal
    /// space (any port), while still allowing public hosts through.
    #[test]
    fn deny_net_blocks_literal_internal_ip_urls() {
        let mut perms = base_permissions();
        let denied = [
            "http://127.0.0.1:5001/api/v0/pin/add",     // kubo IPFS daemon
            "http://169.254.169.254/latest/meta-data/", // cloud metadata
            "http://10.0.0.5:8080/",
            "http://172.16.0.1/",
            "http://192.168.1.1/",
            "http://[::1]:5001/",
            "https://127.0.0.1/",
        ];
        for url_str in denied {
            let url = url::Url::parse(url_str).unwrap();
            assert!(
                perms.check_net_url(&url, "fetch()").is_err(),
                "{url_str} must be denied by deny_net"
            );
        }

        let allowed = [
            "https://example.com/",
            "https://8.8.8.8/",
            "http://1.1.1.1:443/",
        ];
        for url_str in allowed {
            let url = url::Url::parse(url_str).unwrap();
            assert!(
                perms.check_net_url(&url, "fetch()").is_ok(),
                "{url_str} must be allowed"
            );
        }
    }

    /// Regression for F-01: literal IPv6-range URLs (ULA `fc00::/7` incl. the AWS
    /// IMDSv6 metadata address, and link-local `fe80::/10`) must be denied at the
    /// permission layer. `deny_net`'s string parser can't express these subnets,
    /// so they only get blocked via the `Host::IpSubnet` descriptors that
    /// `base_net_permission` adds. Public IPv6 must still be allowed.
    #[test]
    fn deny_net_blocks_literal_internal_ipv6_range_urls() {
        let mut perms = base_permissions();
        let denied = [
            "http://[fc00::1]/",           // ULA
            "http://[fd00:ec2::254]/",     // AWS IMDSv6 metadata
            "http://[fd12:3456::1]:8080/", // ULA with port
            "http://[fe80::1]/",           // link-local
        ];
        for url_str in denied {
            let url = url::Url::parse(url_str).unwrap();
            assert!(
                perms.check_net_url(&url, "fetch()").is_err(),
                "{url_str} must be denied by the IPv6-subnet deny descriptors"
            );
        }

        // Public IPv6 (Cloudflare/Google DNS) must remain reachable.
        for url_str in ["https://[2606:4700:4700::1111]/", "https://[2001:4860:4860::8888]/"] {
            let url = url::Url::parse(url_str).unwrap();
            assert!(
                perms.check_net_url(&url, "fetch()").is_ok(),
                "{url_str} must be allowed"
            );
        }
    }

    /// `base_net_permission` must both allow all public hosts and deny the full
    /// internal set (IPv4 CIDRs, single IPv6 literals, and IPv6 CIDR subnets).
    #[test]
    fn base_net_permission_denies_full_internal_set() {
        // Every DENY_NET string entry must have produced a deny rule…
        let mut perms = base_permissions();
        for entry in DENY_NET {
            // Turn a descriptor like "10.0.0.0/8" or "[::1]" into a probe URL.
            let probe = match *entry {
                "0.0.0.0/8" => "http://0.1.2.3/",
                "10.0.0.0/8" => "http://10.1.2.3/",
                "100.64.0.0/10" => "http://100.64.0.1/",
                "127.0.0.0/8" => "http://127.0.0.1/",
                "169.254.0.0/16" => "http://169.254.169.254/",
                "172.16.0.0/12" => "http://172.16.0.1/",
                "192.168.0.0/16" => "http://192.168.0.1/",
                "198.18.0.0/15" => "http://198.18.0.1/",
                "[::]" => "http://[::]/",
                "[::1]" => "http://[::1]/",
                other => panic!("unhandled DENY_NET entry {other}; add a probe"),
            };
            let url = url::Url::parse(probe).unwrap();
            assert!(
                perms.check_net_url(&url, "fetch()").is_err(),
                "{probe} (from {entry}) must be denied"
            );
        }
    }

    #[test]
    fn cidr_contains_matches_v4_ranges() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert!(cidr_contains("127.0.0.0/8", ip("127.0.0.1")));
        assert!(cidr_contains("127.0.0.0/8", ip("127.9.9.9")));
        assert!(!cidr_contains("127.0.0.0/8", ip("128.0.0.1")));
        assert!(cidr_contains("10.0.0.0/8", ip("10.1.2.3")));
        assert!(!cidr_contains("10.0.0.0/8", ip("11.0.0.1")));
        assert!(cidr_contains("172.16.0.0/12", ip("172.31.255.255")));
        assert!(!cidr_contains("172.16.0.0/12", ip("172.32.0.1")));
        // bracketed single IPv6 literals
        assert!(cidr_contains("[::1]", ip("::1")));
        assert!(!cidr_contains("[::1]", ip("::2")));
        // cross-family never matches
        assert!(!cidr_contains("127.0.0.0/8", ip("::1")));
    }

    /// With no allowlist configured (production default), the effective deny_net
    /// is exactly the full DENY_NET.
    #[test]
    fn effective_deny_net_is_full_without_allowlist() {
        // NB: relies on LIT_ACTIONS_EGRESS_ALLOWLIST being unset in the test env.
        assert_eq!(
            effective_deny_net(),
            DENY_NET.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );
    }

    /// Self-guarding: only asserts when the allowlist env is actually set (run
    /// via `LIT_ACTIONS_EGRESS_ALLOWLIST=127.0.0.1 cargo test ...
    /// allowlist_exempts_loopback`). Verifies both layers exempt the IP: the
    /// resolver-level `is_forbidden_ip` and the permission-level deny_net (the
    /// covering `127.0.0.0/8` entry is dropped) — this is exactly the keychain
    /// runtime-fixture path.
    #[test]
    fn allowlist_exempts_loopback() {
        let loopback: IpAddr = "127.0.0.1".parse().unwrap();
        if !lit_actions_ext::egress::is_egress_allowlisted(loopback) {
            return; // env not set for this run; nothing to assert
        }
        assert!(
            !is_forbidden_ip(loopback),
            "allowlisted IP must not be forbidden"
        );
        assert!(
            !effective_deny_net().iter().any(|e| e == "127.0.0.0/8"),
            "deny_net must drop the entry covering an allowlisted IP"
        );
    }
}
