//! Grant signing: EIP-191 personal-sign over the canonical grant JSON, so the
//! reader action can verify with `ethers.utils.verifyMessage`.

use alloy_primitives::{eip191_hash_message, Address};
use anyhow::{Context, Result};
use k256::ecdsa::SigningKey;

#[derive(Clone)]
pub struct GrantSigner {
    key: SigningKey,
    address: Address,
}

impl GrantSigner {
    pub fn from_hex(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        let stripped = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .unwrap_or(trimmed);
        let bytes = hex::decode(stripped).context("GRANT_SIGNING_KEY must be hex")?;
        if bytes.len() != 32 {
            anyhow::bail!(
                "GRANT_SIGNING_KEY must be 32 bytes ({} given). Generate one with `openssl rand -hex 32`.",
                bytes.len()
            );
        }
        let key = SigningKey::from_slice(&bytes)
            .context("GRANT_SIGNING_KEY is not a valid secp256k1 scalar")?;
        let point = key.verifying_key().to_encoded_point(false);
        let address = Address::from_raw_public_key(&point.as_bytes()[1..]);
        Ok(Self { key, address })
    }

    /// EIP-55 checksummed signer address. Baked into the reader action source.
    pub fn address(&self) -> String {
        self.address.to_checksum(None)
    }

    /// 65-byte `r || s || v` signature, 0x-hex, with `v ∈ {27, 28}` as ethers expects.
    pub fn sign_message(&self, message: &str) -> Result<String> {
        let digest = eip191_hash_message(message.as_bytes());
        let (sig, recid) = self
            .key
            .sign_prehash_recoverable(digest.as_slice())
            .context("grant signing failed")?;
        let mut out = [0u8; 65];
        out[..64].copy_from_slice(&sig.to_bytes());
        out[64] = 27 + recid.to_byte();
        Ok(format!("0x{}", hex::encode(out)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};

    const KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";

    #[test]
    fn derives_a_stable_address() {
        let s = GrantSigner::from_hex(KEY).unwrap();
        // Well-known test vector for this private key.
        assert_eq!(s.address(), "0x2c7536E3605D9C16a7a3D7b1898e529396a65c23");
        assert_eq!(
            GrantSigner::from_hex(&KEY[2..]).unwrap().address(),
            s.address()
        );
    }

    #[test]
    fn signature_recovers_to_signer() {
        let s = GrantSigner::from_hex(KEY).unwrap();
        let msg = "{\"v\":1,\"name\":\"OPENAI_API_KEY\"}";
        let sig_hex = s.sign_message(msg).unwrap();
        let bytes = hex::decode(&sig_hex[2..]).unwrap();
        assert_eq!(bytes.len(), 65);
        let sig = Signature::from_slice(&bytes[..64]).unwrap();
        let recid = RecoveryId::from_byte(bytes[64] - 27).unwrap();
        let digest = eip191_hash_message(msg.as_bytes());
        let vk = VerifyingKey::recover_from_prehash(digest.as_slice(), &sig, recid).unwrap();
        let point = vk.to_encoded_point(false);
        let recovered = Address::from_raw_public_key(&point.as_bytes()[1..]);
        assert_eq!(recovered.to_checksum(None), s.address());
    }

    #[test]
    fn rejects_bad_keys() {
        assert!(GrantSigner::from_hex("0xzz").is_err());
        assert!(GrantSigner::from_hex("0x0011").is_err());
        assert!(GrantSigner::from_hex(&"00".repeat(32)).is_err());
    }
}
