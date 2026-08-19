//! Lit Chat — private web chat served from a TEE.
//!
//! Implementation of `plans/tee-chat-app.md`. Two binaries share this
//! library: `lit-chat` (consumer app: chat UI, SSE OpenRouter proxy,
//! envelope-encrypted storage) and `lit-chat-admin` (admin console:
//! provider-key custody and rotation, spend caps, circuit breaker, audit).
//!
//! Ground rules enforced throughout:
//! - Content is ciphertext everywhere outside the enclave (AES-256-GCM with
//!   AAD binding; keys derived on demand via dstack `get_key`, never stored).
//! - Sessions and magic-link identity are TEE-signed, never DB-rooted.
//! - Logging is content-free: no message bodies, titles, emails, or raw
//!   user_refs in any log line. Hashes and UUIDs only.

pub mod admin;
pub mod attestation;
pub mod config;
pub mod crypto;
pub mod db;
pub mod envelope;
pub mod headers;
pub mod identity;
pub mod mail;
pub mod metering;
pub mod openrouter;
pub mod rate_limit;
pub mod routes;
pub mod session;
pub mod store;

use anyhow::Result;
use crypto::keys::KeySource;

/// Service-level MAC/KEK material, derived once at boot from the CVM's own
/// dstack app root. User KEKs are NOT here — they are derived per request
/// from the caller's session-bound `user_ref` and never cached.
#[derive(Clone)]
pub struct Keyring {
    /// MACs consumer session tokens ("chat/v1/session-mac").
    pub session_mac: [u8; 32],
    /// MACs magic-link identity tokens ("chat/v1/magic-link-mac").
    pub magic_link_mac: [u8; 32],
    /// MACs admin session tokens — separate key so consumer and admin
    /// sessions are unforgeable across each other ("chat/v1/admin-session-mac").
    pub admin_session_mac: [u8; 32],
    /// MACs admin roster rows ("chat/v1/admin-roster-mac").
    pub admin_roster_mac: [u8; 32],
    /// MACs + chains audit rows ("chat/v1/audit-mac").
    pub audit_mac: [u8; 32],
    /// Service KEK for the encrypted provider-key store ("chat/v1/provider-keys").
    pub provider_kek: [u8; 32],
    /// Service KEK for admin WebAuthn credential blobs ("chat/v1/admin-cred").
    pub cred_kek: [u8; 32],
    /// HKDF namespace for email -> user_ref derivation ("chat/v1/user-id-namespace").
    pub user_id_namespace: [u8; 32],
}

impl Keyring {
    pub async fn load(source: &KeySource) -> Result<Self> {
        Ok(Self {
            session_mac: source.get_key("chat/v1/session-mac", "chat-svc").await?,
            magic_link_mac: source.get_key("chat/v1/magic-link-mac", "chat-svc").await?,
            admin_session_mac: source
                .get_key("chat/v1/admin-session-mac", "chat-svc")
                .await?,
            admin_roster_mac: source
                .get_key("chat/v1/admin-roster-mac", "chat-svc")
                .await?,
            audit_mac: source.get_key("chat/v1/audit-mac", "chat-svc").await?,
            provider_kek: source.get_key("chat/v1/provider-keys", "chat-svc").await?,
            cred_kek: source.get_key("chat/v1/admin-cred", "chat-svc").await?,
            user_id_namespace: source
                .get_key("chat/v1/user-id-namespace", "chat-svc")
                .await?,
        })
    }
}

/// Derive the caller's KEK. On demand, never stored, never leaves the enclave.
pub async fn user_kek(source: &KeySource, user_ref: &str) -> Result<[u8; 32]> {
    let path = format!("chat/v1/user/{user_ref}");
    source.get_key(&path, "chat-kek").await
}
