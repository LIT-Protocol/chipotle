//! Enclave key derivation.
//!
//! Production: dstack `get_key(path, purpose)` against the chat CVM's own
//! socket, keccak-wrapped per the platform discipline (the raw derived key is
//! never used directly because dstack can also export its signature chain —
//! see lit-api-server/src/dstack/v1/mod.rs).
//!
//! Dev/test: a deterministic keccak KDF over a developer-supplied 32-byte
//! master key, mirroring lit-actions/local-cli/src/keys.rs. Values will not
//! match production; the surface is identical. Compiled out under
//! `--features production`.

#[cfg(not(feature = "production"))]
use anyhow::Context;
use anyhow::{anyhow, Result};

const DSTACK_SOCKET_DEFAULT: &str = "/var/run/dstack.sock";

#[derive(Clone)]
pub enum KeySource {
    Dstack,
    #[cfg(not(feature = "production"))]
    DevMaster([u8; 32]),
}

impl KeySource {
    /// Production builds always use the dstack socket. Non-production builds
    /// accept `LIT_CHAT_DEV_MASTER_KEY` (base64, 32 bytes) for offline dev.
    pub fn from_env() -> Result<Self> {
        #[cfg(not(feature = "production"))]
        {
            if let Ok(raw) = std::env::var("LIT_CHAT_DEV_MASTER_KEY") {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(raw.trim())
                    .context("LIT_CHAT_DEV_MASTER_KEY must be base64")?;
                let key: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| anyhow!("LIT_CHAT_DEV_MASTER_KEY must decode to 32 bytes"))?;
                return Ok(KeySource::DevMaster(key));
            }
        }
        Ok(KeySource::Dstack)
    }

    pub async fn get_key(&self, path: &str, purpose: &str) -> Result<[u8; 32]> {
        match self {
            KeySource::Dstack => dstack_get_key(path, purpose).await,
            #[cfg(not(feature = "production"))]
            KeySource::DevMaster(master) => Ok(dev_derive(master, purpose, path)),
        }
    }
}

fn resolve_socket_path() -> String {
    #[cfg(feature = "production")]
    {
        DSTACK_SOCKET_DEFAULT.to_string()
    }
    #[cfg(not(feature = "production"))]
    {
        std::env::var("DSTACK_SOCKET").unwrap_or_else(|_| DSTACK_SOCKET_DEFAULT.to_string())
    }
}

async fn dstack_get_key(path: &str, purpose: &str) -> Result<[u8; 32]> {
    let socket_path = resolve_socket_path();
    if !std::path::Path::new(&socket_path).exists() {
        return Err(anyhow!(
            "dstack socket not found at {socket_path} — not running inside a dstack-enabled TEE; \
             is the simulator running?"
        ));
    }
    let client = dstack_sdk::dstack_client::DstackClient::new(Some(socket_path.as_str()));
    let resp = client
        .get_key(Some(path.to_string()), Some(purpose.to_string()))
        .await
        .map_err(|e| anyhow!("dstack get_key failed: {e}"))?;
    let secret = resp
        .decode_key()
        .map_err(|e| anyhow!("failed to decode dstack key: {e}"))?;
    if secret.len() != 32 {
        return Err(anyhow!("dstack secret wrong length: {}", secret.len()));
    }
    // Keccak-wrap so exporting one derived secret can never weaken another
    // (same rationale as lit-api-server dstack::v1::get_key).
    Ok(super::keccak256(&secret))
}

#[cfg(not(feature = "production"))]
fn dev_derive(master: &[u8; 32], purpose: &str, path: &str) -> [u8; 32] {
    let mut preimage = Vec::with_capacity(master.len() + purpose.len() + path.len() + 2);
    preimage.extend_from_slice(master);
    preimage.push(0);
    preimage.extend_from_slice(purpose.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(path.as_bytes());
    super::keccak256(&preimage)
}

#[cfg(all(test, not(feature = "production")))]
mod tests {
    use super::*;

    #[test]
    fn dev_derivation_is_deterministic_and_domain_separated() {
        let master = [3u8; 32];
        let a = dev_derive(&master, "chat-kek", "chat/v1/user/x");
        let b = dev_derive(&master, "chat-kek", "chat/v1/user/x");
        let c = dev_derive(&master, "chat-svc", "chat/v1/user/x");
        let d = dev_derive(&master, "chat-kek", "chat/v1/user/y");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }
}
