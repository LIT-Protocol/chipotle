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
