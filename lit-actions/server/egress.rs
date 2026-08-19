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
use std::net::SocketAddr;
use std::sync::Arc;

use deno_runtime::deno_fetch::dns::{Resolve, Resolver, Resolving};
use hyper_util::client::legacy::connect::dns::Name;
use lit_actions_ext::egress::is_forbidden_ip;

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
/// Only IPv4 CIDRs and *single* bracketed IPv6 literals are listed: Deno's
/// net-descriptor parser (v0.107) does not accept IPv6 CIDR subnets — it splits
/// host from port on `:`, which mangles any bare/subnet IPv6 form. IPv6 *ranges*
/// (link-local `fe80::/10`, unique-local `fc00::/7`) are therefore enforced only
/// through the resolver ([`is_forbidden_ip`]), which is comprehensive for
/// hostname fetches. The residual gap is a literal IPv6 *range* URL such as
/// `http://[fc00::1]/`; there are no known internal IPv6 services (every vector
/// in CPL-295 is IPv4), so this is accepted as a low-risk limitation of the
/// upstream parser rather than worked around.
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
        let perms = Permissions::from_options(
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
}
