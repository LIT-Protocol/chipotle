//! Balance-cache invalidation for the other instances of this app.
//!
//! Each instance caches Stripe balances in its own process. When more than one
//! instance serves traffic (one per hostname during a platform migration), a
//! top-up confirmed on one would otherwise stay invisible on the others until
//! their cache expires, and a customer who just paid would keep getting 402.
//! After a top-up this instance tells its peers to drop the entry, through the
//! same authenticated endpoint `lit-payments` uses.
//!
//! Peers come from `LIT_PEER_BASE_URLS` (comma-separated `https` origins).
//! Unset, which is right whenever a single instance serves traffic, this does
//! nothing. A peer that cannot be reached is logged and otherwise ignored: its
//! cache still expires by itself.

use std::time::Duration;

pub const PEERS_ENV: &str = "LIT_PEER_BASE_URLS";
const TIMEOUT: Duration = Duration::from_secs(5);

/// Parses a comma-separated list of peer origins, dropping blanks and any
/// entry that is not a plain `https` URL (the shared secret is sent to them).
pub fn parse_peers(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|entry| entry.trim().trim_end_matches('/'))
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let valid = entry.strip_prefix("https://").is_some_and(|rest| {
                !rest.is_empty()
                    && rest
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
            });
            if !valid {
                // Not echoed: a malformed entry may be malformed because it
                // carries a credential or token.
                tracing::error!("a {PEERS_ENV} entry is not a plain https origin and is ignored");
            }
            valid.then(|| entry.to_string())
        })
        .collect()
}

/// Tells every configured peer to drop its cached balance for `customer_id`.
/// Returns immediately; the calls run in the background.
pub fn notify_balance_invalidated(customer_id: &str) {
    let peers = parse_peers(&std::env::var(PEERS_ENV).unwrap_or_default());
    if peers.is_empty() {
        return;
    }
    let secret = std::env::var("LIT_INTERNAL_SHARED_SECRET").unwrap_or_default();
    if secret.trim().is_empty() {
        tracing::warn!(
            "{PEERS_ENV} is set but LIT_INTERNAL_SHARED_SECRET is not; peers not notified"
        );
        return;
    }
    let body = serde_json::json!({ "customer_id": customer_id });
    tokio::spawn(async move {
        // No redirects: the shared secret is a custom header, which a redirect
        // to another origin would carry along.
        let client = match reqwest::Client::builder()
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
        {
            Ok(client) => client,
            Err(e) => {
                tracing::warn!("peer invalidation client could not be built: {e}");
                return;
            }
        };
        for peer in peers {
            let outcome = client
                .post(format!("{peer}/internal/invalidate_balance_cache"))
                .header("X-Internal-Secret", secret.trim())
                .json(&body)
                .send()
                .await;
            match outcome {
                Ok(response) if response.status().is_success() => {}
                // Never log the response body: a misrouted request could echo the secret.
                Ok(response) => tracing::warn!(
                    "peer {peer} did not invalidate its balance cache: HTTP {}",
                    response.status()
                ),
                Err(e) => tracing::warn!(
                    "peer {peer} could not be told to invalidate its balance cache: {}",
                    e.without_url()
                ),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::parse_peers;

    #[test]
    fn peers_are_plain_https_origins() {
        assert!(parse_peers("").is_empty());
        assert!(parse_peers(" , ").is_empty());
        assert_eq!(
            parse_peers(" https://a.example/ , https://b.example:8443 ,"),
            ["https://a.example", "https://b.example:8443"]
        );
        // The secret must never go over plain HTTP or to a URL with a path,
        // credentials or query.
        for bad in [
            "http://a.example",
            "a.example",
            "https://",
            "https://a.example/path",
            "https://user@a.example",
            "https://a.example?x=1",
        ] {
            assert!(parse_peers(bad).is_empty(), "{bad:?}");
        }
    }
}
