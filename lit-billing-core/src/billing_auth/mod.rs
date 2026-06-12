//! Shared billing auth primitives for `lit-api-server` and `lit-payments`.
//!
//! The dashboard and external API clients authenticate with either:
//!
//! 1. **API key** (`Authorization: Bearer <key>` or `X-Api-Key: <key>`) — the
//!    legacy path. The raw key string is the identity until something resolves
//!    it to a wallet (and from there to a Stripe customer).
//!
//! 2. **Wallet signature** — an EIP-712 typed-data payload, base64-JSON-encoded
//!    in the `X-Wallet-Auth` header, with `primaryType: "BillingAuth"`.
//!    Recovering the signer proves wallet possession.
//!
//! Both services need the same identity model. The Rocket request guard
//! [`BillingAuth`] lives here; per-service plumbing (on-chain key resolver,
//! local EIP-712 verifier) is supplied via the [`AuthResolver`] trait that
//! handlers pull from Rocket state.
//!
//! ## History
//!
//! Originally lived in a separate `lit-billing-auth` crate. Folded into
//! `lit-billing-core` after glitch's PR #448 review: the auth pieces and
//! the Stripe pieces ride together in every consumer (lit-api-server,
//! lit-payments), so two parallel crates was needless ceremony.

pub mod guard;
pub mod resolver;

pub use guard::BillingAuth;
pub use resolver::{AuthError, AuthResolver, ResolvedIdentity, WalletAuthPayload};

// Re-export the precomputed-hash-shape detector from the on-chain module —
// it's the same function the original `lit-billing-auth` crate exposed at
// its root. Keeping the import path stable so callers (lit-payments,
// lit-api-server) don't have to chase the move.
pub use crate::on_chain::is_precomputed_hash_shape;
