//! Shape checks for Stripe object ids and request paths.
//!
//! [`StripeClient`](crate::StripeClient) builds every request URL as
//! `<base>/<path>`, and callers interpolate object ids into `path`. reqwest
//! parses the result as a WHATWG URL, which resolves `.` and `..` segments
//! (including percent-encoded ones) and treats `?` and `#` as delimiters. An
//! id such as `x/../pi_123` therefore addresses `pi_123` at Stripe while
//! comparing unequal to `pi_123` as a string. Any code that dedups on the
//! string (a database key, an idempotency key) can be made to act twice on
//! one object. Stripe paths and ids use only ASCII letters, digits and `_`,
//! so anything else is refused.

/// Upper bound on a Stripe id. Stripe documents ids as at most 255 characters.
const MAX_ID_LEN: usize = 255;

/// Whether `id` is a canonical PaymentIntent id: `pi_` followed by ASCII
/// letters and digits only.
pub fn is_payment_intent_id(id: &str) -> bool {
    id.len() <= MAX_ID_LEN
        && id
            .strip_prefix("pi_")
            .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Whether `path` is a plain Stripe API path: one or more `/`-separated,
/// non-empty segments of ASCII letters, digits and `_`.
///
/// This refuses dot segments, percent-encoding, query and fragment
/// delimiters, whitespace and leading, trailing or doubled slashes.
pub fn is_plain_stripe_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spellings that reach the same PaymentIntent at Stripe, or that change the
    /// request, while comparing unequal to the canonical id.
    const ALIASES: &[&str] = &[
        "x/../pi_3MtwBwLkdIwHu7ix28a3tqPa",
        "./pi_3MtwBwLkdIwHu7ix28a3tqPa",
        "%2e/pi_3MtwBwLkdIwHu7ix28a3tqPa",
        "pi_3MtwBwLkdIwHu7ix28a3tqPa/.",
        "pi_3MtwBwLkdIwHu7ix28a3tqPa/",
        "pi_3MtwBwLkdIwHu7ix28a3tqPa?expand[]=customer",
        "pi_3MtwBwLkdIwHu7ix28a3tqPa#x",
        "%70i_3MtwBwLkdIwHu7ix28a3tqPa",
        "pi_3MtwBwLkdIwHu7ix28a3tqP%61",
        " pi_3MtwBwLkdIwHu7ix28a3tqPa",
        "pi_3MtwBwLkdIwHu7ix28a3tqPa ",
        "pi_3MtwBwLkdIwHu7ix28a3tqPa\n",
        "pi_3MtwBwLkdIwHu7ix28a3tqPa\\",
    ];

    #[test]
    fn canonical_payment_intent_ids_are_accepted() {
        assert!(is_payment_intent_id("pi_3MtwBwLkdIwHu7ix28a3tqPa"));
        assert!(is_payment_intent_id("pi_1"));
        assert!(is_payment_intent_id(&format!(
            "pi_{}",
            "a".repeat(MAX_ID_LEN - 3)
        )));
    }

    #[test]
    fn non_canonical_payment_intent_ids_are_refused() {
        for id in ALIASES {
            assert!(!is_payment_intent_id(id), "accepted {id:?}");
        }
        for id in [
            "",
            "pi_",
            "PI_3MtwBwLkdIwHu7ix28a3tqPa",
            "pm_3MtwBwLkdIwHu7ix28a3tqPa",
            "pi_3Mtw_BwLk",
            "pi_3Mtw-BwLk",
            "pi_\u{0661}\u{0662}\u{0663}",
            "pi_3MtwBwLk\u{0}",
        ] {
            assert!(!is_payment_intent_id(id), "accepted {id:?}");
        }
        assert!(!is_payment_intent_id(&format!(
            "pi_{}",
            "a".repeat(MAX_ID_LEN - 2)
        )));
    }

    #[test]
    fn every_path_the_services_use_is_plain() {
        for path in [
            "balance",
            "customers",
            "customers/search",
            "customers/cus_Abc123",
            "customers/cus_Abc123/balance_transactions",
            "invoiceitems",
            "invoices",
            "invoices/in_Abc123/finalize",
            "invoices/in_Abc123/send",
            "payment_intents",
            "payment_intents/pi_Abc123",
            "payment_methods",
            "payment_methods/pm_Abc123",
            "setup_intents",
            "setup_intents/seti_Abc123",
        ] {
            assert!(is_plain_stripe_path(path), "refused {path:?}");
        }
    }

    #[test]
    fn aliasing_paths_are_refused() {
        for id in ALIASES {
            let path = format!("payment_intents/{id}");
            assert!(!is_plain_stripe_path(&path), "accepted {path:?}");
        }
        for path in [
            "",
            "/customers",
            "customers/",
            "customers//cus_1",
            "customers/../balance",
        ] {
            assert!(!is_plain_stripe_path(path), "accepted {path:?}");
        }
    }

    /// Documents the behaviour the checks defend against, with the pinned
    /// reqwest/url versions: the aliased URL addresses the canonical object.
    #[test]
    fn reqwest_url_parsing_collapses_dot_segments() {
        let base = crate::http::stripe_base();
        for (alias, expected) in [
            ("x/../pi_123", "/v1/payment_intents/pi_123"),
            ("./pi_123", "/v1/payment_intents/pi_123"),
            ("%2e/pi_123", "/v1/payment_intents/pi_123"),
            ("a/%2e%2e/pi_123", "/v1/payment_intents/pi_123"),
        ] {
            let url = reqwest::Url::parse(&format!("{base}/payment_intents/{alias}")).unwrap();
            assert_eq!(url.path(), expected, "{alias:?}");
        }
    }
}
