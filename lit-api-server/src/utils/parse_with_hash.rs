use crate::core::v1::helpers::api_status::ApiStatus;
use ethers::{
    types::{H160, U256},
    utils::keccak256,
};
use lit_core::utils::binary::hex_to_bytes;

pub fn api_key_hash(api_key_base_64: &str) -> U256 {
    U256::from_big_endian(&keccak256(api_key_base_64.as_bytes()))
}

/// Hash a usage API key string, OR pass through a pre-computed keccak256 hash.
/// If `s` is a 0x-prefixed 66-character hex string (32 bytes), it is treated as
/// an already-computed hash and parsed directly. Otherwise it is keccak256-hashed.
pub fn usage_api_key_to_hash(s: &str) -> U256 {
    let trimmed = s.trim();
    if ((trimmed.starts_with("0x") || trimmed.starts_with("0X")) && trimmed.len() == 66)
        && let Ok(bytes) = hex_to_bytes(trimmed)
        && bytes.len() == 32
    {
        return U256::from_big_endian(&bytes);
    }
    api_key_hash(trimmed)
}

pub fn hex_array_to_u256_array(hex_array: &[String]) -> Result<Vec<U256>, ApiStatus> {
    parse_u256_hex_list(hex_array)
}

#[allow(dead_code)]
pub fn hex_array_to_h160_array(hex_array: &[String]) -> Result<Vec<H160>, ApiStatus> {
    parse_h160_hex_list(hex_array)
}

pub fn string_to_hashed_u256(s: &str) -> Result<U256, ApiStatus> {
    parse_u256(s)
}

pub fn pkp_id_to_h160(s: &str) -> Result<H160, ApiStatus> {
    wallet_string_to_h160(s)
}

pub fn wallet_string_to_h160(s: &str) -> Result<H160, ApiStatus> {
    if !(s.starts_with("0x") || s.starts_with("0X")) {
        return Err(ApiStatus::bad_request(
            anyhow::anyhow!("H160 address must be prefixed with 0x: {}", s),
            "Unable to parse H160 address",
        ));
    }

    parse_h160(s).map_err(|e| {
        ApiStatus::bad_request(
            anyhow::anyhow!("Unable to parse wallet address: {}", e),
            "Unable to parse wallet address",
        )
    })
}

pub fn ipfs_cid_to_u256(ipfs_id: &str) -> Result<U256, ApiStatus> {
    string_to_bytes_to_hash_to_u256(ipfs_id).map_err(|e| {
        ApiStatus::bad_request(
            anyhow::anyhow!("Unable to parse IPFS ID: {}", e),
            "Parsing error.",
        )
    })
}

pub fn string_group_id_to_u256(group_id: &str) -> Result<U256, ApiStatus> {
    parse_u256(group_id).map_err(|e| {
        ApiStatus::bad_request(
            anyhow::anyhow!("Unable to parse group ID: {}", e),
            "Unable to parse group ID",
        )
    })
}

/// Parse an already-hashed CID (0x-prefixed hex or decimal string) into U256.
/// Unlike `ipfs_cid_to_u256`, this does NOT keccak256-hash the input.
pub fn hashed_cid_to_u256(hashed_cid: &str) -> Result<U256, ApiStatus> {
    parse_u256(hashed_cid).map_err(|e| {
        ApiStatus::bad_request(
            anyhow::anyhow!("Unable to parse hashed CID: {}", e),
            "Unable to parse hashed CID",
        )
    })
}

fn string_to_bytes_to_hash_to_u256(s: &str) -> Result<U256, ApiStatus> {
    let bytes = keccak256(s.as_bytes());
    Ok(U256::from_big_endian(&bytes))
}

/// Parse U256 from a `0x`-prefixed hex string or a decimal string.
/// Inputs without a `0x`/`0X` prefix are treated as base-10 decimal;
/// hex strings must include the prefix.
fn parse_u256(s: &str) -> Result<U256, ApiStatus> {
    let s = s.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        let bytes = hex_to_bytes(s)
            .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "invalid hex for U256"))?;
        Ok(U256::from_big_endian(&bytes))
    } else {
        U256::from_dec_str(s)
            .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "invalid decimal for U256"))
    }
}

// Parse H160 from hex string (with or without 0x prefix).
fn parse_h160(s: &str) -> Result<H160, ApiStatus> {
    let s = s.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        let bytes = hex_to_bytes(s)
            .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "invalid hex for H160"))?;
        if bytes.len() != 20 {
            return Err(ApiStatus::bad_request(
                anyhow::anyhow!("invalid H160: expected 20 bytes, got {}", bytes.len()),
                "invalid H160: address must be exactly 20 bytes",
            ));
        }
        Ok(H160::from_slice(&bytes))
    } else {
        Err(ApiStatus::bad_request(
            anyhow::anyhow!("invalid hex for H160"),
            "invalid hex for H160",
        ))
    }
}

/// Parse vec of hex strings to Vec<U256> (for permitted_actions / pkps hashes).
fn parse_u256_hex_list(strings: &[String]) -> Result<Vec<U256>, ApiStatus> {
    strings
        .iter()
        .map(|s| {
            let bytes = hex_to_bytes(s.trim())
                .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "invalid hex in list"))?;
            Ok(U256::from_big_endian(&bytes))
        })
        .collect::<Result<Vec<_>, _>>()
}

fn parse_h160_hex_list(strings: &[String]) -> Result<Vec<H160>, ApiStatus> {
    strings
        .iter()
        .map(|s| {
            let bytes = hex_to_bytes(s.trim())
                .map_err(|e| ApiStatus::bad_request(anyhow::anyhow!(e), "invalid hex in list"))?;
            if bytes.len() != 20 {
                return Err(ApiStatus::bad_request(
                    anyhow::anyhow!("invalid H160: expected 20 bytes, got {}", bytes.len()),
                    "invalid H160: address must be exactly 20 bytes",
                ));
            }
            Ok(H160::from_slice(&bytes))
        })
        .collect::<Result<Vec<_>, _>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── api_key_hash ────────────────────────────────────────────────────
    #[test]
    fn api_key_hash_deterministic() {
        let h1 = api_key_hash("my-secret-key");
        let h2 = api_key_hash("my-secret-key");
        assert_eq!(h1, h2);
    }

    #[test]
    fn api_key_hash_different_inputs() {
        assert_ne!(api_key_hash("key-a"), api_key_hash("key-b"));
    }

    // ── usage_api_key_to_hash ───────────────────────────────────────────
    #[test]
    fn usage_api_key_to_hash_plain_key_hashes() {
        let hash = usage_api_key_to_hash("my-api-key");
        assert_eq!(hash, api_key_hash("my-api-key"));
    }

    #[test]
    fn usage_api_key_to_hash_precomputed_hex_passthrough() {
        // A valid 0x-prefixed 32-byte hex string should be parsed directly, not re-hashed.
        let hex_str = "0x0000000000000000000000000000000000000000000000000000000000000001";
        let result = usage_api_key_to_hash(hex_str);
        assert_eq!(result, U256::from(1));
    }

    #[test]
    fn usage_api_key_to_hash_trims_whitespace() {
        let hex_str = "  0x0000000000000000000000000000000000000000000000000000000000000002  ";
        assert_eq!(usage_api_key_to_hash(hex_str), U256::from(2));
    }

    #[test]
    fn usage_api_key_to_hash_short_hex_hashes_instead() {
        // A short 0x string is not a precomputed hash; it should be keccak256-hashed.
        let result = usage_api_key_to_hash("0xabcd");
        assert_eq!(result, api_key_hash("0xabcd"));
    }

    // ── parse_u256 (via string_to_hashed_u256 / hashed_cid_to_u256) ────
    #[test]
    fn parse_u256_decimal() {
        let result = hashed_cid_to_u256("42").unwrap();
        assert_eq!(result, U256::from(42));
    }

    #[test]
    fn parse_u256_hex() {
        let result = hashed_cid_to_u256("0xff").unwrap();
        assert_eq!(result, U256::from(255));
    }

    #[test]
    fn parse_u256_invalid_decimal() {
        assert!(hashed_cid_to_u256("not_a_number").is_err());
    }

    #[test]
    fn parse_u256_invalid_hex() {
        assert!(hashed_cid_to_u256("0xZZZZ").is_err());
    }

    // ── wallet_string_to_h160 / pkp_id_to_h160 ─────────────────────────
    #[test]
    fn wallet_string_valid_address() {
        let addr = wallet_string_to_h160("0x0000000000000000000000000000000000000001").unwrap();
        assert_eq!(addr, H160::from_low_u64_be(1));
    }

    #[test]
    fn wallet_string_missing_0x_prefix() {
        let err = wallet_string_to_h160("0000000000000000000000000000000000000001");
        assert!(err.is_err());
    }

    #[test]
    fn wallet_string_wrong_length() {
        // Too short (only 19 bytes)
        let err = wallet_string_to_h160("0x00000000000000000000000000000000000001");
        assert!(err.is_err());
    }

    #[test]
    fn pkp_id_valid() {
        let addr = pkp_id_to_h160("0x0000000000000000000000000000000000000042").unwrap();
        assert_eq!(addr, H160::from_low_u64_be(0x42));
    }

    // ── ipfs_cid_to_u256 ───────────────────────────────────────────────
    #[test]
    fn ipfs_cid_deterministic() {
        let a = ipfs_cid_to_u256("QmTest123").unwrap();
        let b = ipfs_cid_to_u256("QmTest123").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn ipfs_cid_different_inputs() {
        let a = ipfs_cid_to_u256("QmTestA").unwrap();
        let b = ipfs_cid_to_u256("QmTestB").unwrap();
        assert_ne!(a, b);
    }

    // ── hex_array_to_u256_array ─────────────────────────────────────────
    #[test]
    fn hex_array_to_u256_valid() {
        let input = vec![
            "0x0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            "0x00000000000000000000000000000000000000000000000000000000000000ff".to_string(),
        ];
        let result = hex_array_to_u256_array(&input).unwrap();
        assert_eq!(result, vec![U256::from(1), U256::from(255)]);
    }

    #[test]
    fn hex_array_to_u256_invalid_entry() {
        let input = vec!["not_hex".to_string()];
        assert!(hex_array_to_u256_array(&input).is_err());
    }

    #[test]
    fn hex_array_to_u256_empty() {
        let result = hex_array_to_u256_array(&[]).unwrap();
        assert!(result.is_empty());
    }

    // ── hex_array_to_h160_array ─────────────────────────────────────────
    #[test]
    fn hex_array_to_h160_valid() {
        let input = vec!["0x0000000000000000000000000000000000000001".to_string()];
        let result = hex_array_to_h160_array(&input).unwrap();
        assert_eq!(result, vec![H160::from_low_u64_be(1)]);
    }

    #[test]
    fn hex_array_to_h160_wrong_length() {
        let input = vec!["0xff".to_string()];
        assert!(hex_array_to_h160_array(&input).is_err());
    }

    // ── string_group_id_to_u256 ─────────────────────────────────────────
    #[test]
    fn string_group_id_valid_decimal() {
        let result = string_group_id_to_u256("100").unwrap();
        assert_eq!(result, U256::from(100));
    }

    #[test]
    fn string_group_id_valid_hex() {
        let result = string_group_id_to_u256("0x64").unwrap();
        assert_eq!(result, U256::from(100));
    }

    #[test]
    fn string_group_id_invalid() {
        assert!(string_group_id_to_u256("abc_not_valid").is_err());
    }
}
