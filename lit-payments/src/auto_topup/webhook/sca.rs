//! SCA recovery-token primitives.
//!
//! Used when an off-session PaymentIntent returns `authentication_required`.
//! We mint a one-time, URL-safe token, persist it on the
//! `auto_topup_config` row (with a 24-hour expiry), email it to the user,
//! and let Phase 7's `/billing/auto_topup_resume` endpoint exchange it for
//! the PI's `client_secret`.
//!
//! The token is the dashboard's only handle on the pending PI — keep it
//! random enough that guessing is infeasible. 32 random bytes (URL-safe
//! base64 encoded, ~43 chars) is well above the practical threshold and
//! matches the existing `auth::session::generate_token` shape.

use base64::Engine;
use rand::RngCore;

pub fn generate_recovery_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn tokens_are_unique_and_url_safe() {
        let mut seen = HashSet::new();
        for _ in 0..200 {
            let tok = generate_recovery_token();
            assert!(!tok.contains('+'));
            assert!(!tok.contains('/'));
            assert!(!tok.contains('='));
            assert!(tok.len() >= 40);
            assert!(seen.insert(tok), "duplicate token");
        }
    }
}
