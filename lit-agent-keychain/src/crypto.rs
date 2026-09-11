use crate::models::Signed;
use anyhow::{bail, Result};
use k256::ecdsa::{signature::hazmat::PrehashVerifier, Signature, VerifyingKey};
use rand::RngCore;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
pub fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn canonical(value: &Value) -> Result<String> {
    fn validate(v: &Value, depth: usize) -> Result<()> {
        if depth > 24 {
            bail!("object too deep");
        }
        match v {
            Value::Number(n)
                if n.as_i64().is_none_or(|x| {
                    !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&x)
                }) =>
            {
                bail!("unsafe number")
            }
            Value::Object(o) => {
                for (key, v) in o {
                    if !key.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                        || !key.bytes().all(|c| c.is_ascii_alphanumeric())
                    {
                        bail!("invalid field name");
                    }
                    validate(v, depth + 1)?;
                }
            }
            Value::Array(a) => {
                for v in a {
                    validate(v, depth + 1)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    validate(value, 0)?;
    // serde_json's default map is a BTreeMap. Do not enable preserve_order.
    Ok(serde_json::to_string(value)?)
}
pub fn digest(value: &Value) -> Result<String> {
    Ok(hash_bytes(canonical(value)?.as_bytes()))
}
pub fn verify_signed(signed: &Signed, public_key: &str, vault: &str) -> Result<()> {
    let p = &signed.receipt.payload;
    if p.v != 2
        || p.domain != "lit-keychain/receipt/v2"
        || p.vault_id != vault
        || p.object_hash != digest(&signed.document)?
        || signed.document.get("vaultId").and_then(Value::as_str) != Some(vault)
        || p.issued_at < 0
        || p.issued_at > time::OffsetDateTime::now_utc().unix_timestamp() + 30
    {
        bail!("invalid receipt binding");
    }
    let key = VerifyingKey::from_sec1_bytes(&hex::decode(public_key.trim_start_matches("0x"))?)?;
    let signature = Signature::from_slice(&hex::decode(&signed.receipt.signature)?)?;
    if signature.normalize_s().is_some() {
        bail!("non-canonical signature");
    }
    key.verify_prehash(
        &hex::decode(digest(&serde_json::to_value(p)?)?)?,
        &signature,
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn canonical_vectors() {
        assert_eq!(
            canonical(&json!({"z": [true, null, "é\n"], "a": 12})).unwrap(),
            "{\"a\":12,\"z\":[true,null,\"é\\n\"]}"
        );
        assert!(canonical(&json!({"a": 1.5})).is_err());
        assert!(canonical(&json!({"a": 9007199254740992u64})).is_err());
        assert!(canonical(&json!({"__proto__": 1})).is_err());
    }
}
