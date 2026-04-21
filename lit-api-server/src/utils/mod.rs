pub mod chain_info;
pub mod parse_with_hash;
use anyhow::Result;
use ethers::types::{H160, U256};
use ethers::utils::keccak256;
use lit_core::utils::binary::bytes_to_hex;
use rand::Rng;

pub fn get_random_secret() -> [u8; 32] {
    let mut secret: [u8; 32] = [0; 32];
    // Get a thread-local random number generator and fill the array.
    rand::thread_rng().fill(&mut secret);
    secret
}

// Generate a unique derivation path for a new wallet.
pub fn generate_unique_derivation_path() -> (U256, String) {
    let seed_bytes = get_random_secret();
    let derivation_bytes = keccak256(seed_bytes);
    let derivation_u256 = U256::from_big_endian(&derivation_bytes);
    // feels redundant, but it's a good way to ensure the derivation path is always the same for a given seed
    let derivation_path = u256_to_derviation_path(derivation_u256);
    (derivation_u256, derivation_path)
}

fn u256_to_bytes(u256: U256) -> [u8; 32] {
    let mut bytes = [0; 32];
    u256.to_big_endian(&mut bytes);
    bytes
}

pub fn u256_to_derviation_path(u256: U256) -> String {
    bytes_to_hex(u256_to_bytes(u256))
}

pub fn generate_lit_action_derivation_path(ipfs_id: &str) -> String {
    let ipfs_id_bytes = ipfs_id.as_bytes();
    let ipfs_id_hash = keccak256(ipfs_id_bytes);
    let ipfs_id_hash_u256 = U256::from_big_endian(&ipfs_id_hash);
    u256_to_derviation_path(ipfs_id_hash_u256)
}

pub fn evm_address_from_public_key(public_key: &str) -> Result<H160> {
    if public_key.len() < 32 {
        return Err(anyhow::anyhow!(
            "Invalid public key length: {}",
            public_key.len()
        ));
    }

    let pkp_address = hex::decode(&public_key.replace("0x", "")[2..])?;
    let pkp_address = keccak256(&pkp_address);
    let pkp_address = H160::from_slice(&pkp_address[12..]);
    Ok(pkp_address)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── u256_to_derviation_path ─────────────────────────────────────────
    #[test]
    fn u256_to_derivation_path_zero() {
        let path = u256_to_derviation_path(U256::zero());
        // bytes_to_hex produces 64 hex chars without "0x" prefix
        assert_eq!(path.len(), 64);
        assert!(path.chars().all(|c| c == '0'));
    }

    #[test]
    fn u256_to_derivation_path_deterministic() {
        let val = U256::from(12345);
        assert_eq!(u256_to_derviation_path(val), u256_to_derviation_path(val));
    }

    // ── generate_unique_derivation_path ─────────────────────────────────
    #[test]
    fn generate_unique_derivation_path_returns_hex() {
        let (u256_val, path) = generate_unique_derivation_path();
        assert_eq!(path.len(), 64);
        assert!(path.chars().all(|c| c.is_ascii_hexdigit()));
        // The path should match the u256 value
        assert_eq!(path, u256_to_derviation_path(u256_val));
    }

    #[test]
    fn generate_unique_derivation_path_is_unique() {
        let (_, path1) = generate_unique_derivation_path();
        let (_, path2) = generate_unique_derivation_path();
        assert_ne!(path1, path2);
    }

    // ── generate_lit_action_derivation_path ─────────────────────────────
    #[test]
    fn lit_action_derivation_path_deterministic() {
        let p1 = generate_lit_action_derivation_path("QmTest123");
        let p2 = generate_lit_action_derivation_path("QmTest123");
        assert_eq!(p1, p2);
    }

    #[test]
    fn lit_action_derivation_path_different_for_different_ids() {
        let p1 = generate_lit_action_derivation_path("QmTestA");
        let p2 = generate_lit_action_derivation_path("QmTestB");
        assert_ne!(p1, p2);
    }

    #[test]
    fn lit_action_derivation_path_format() {
        let path = generate_lit_action_derivation_path("QmTest");
        assert_eq!(path.len(), 64);
        assert!(path.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ── evm_address_from_public_key ─────────────────────────────────────
    #[test]
    fn evm_address_from_public_key_valid() {
        // Uncompressed secp256k1 public key (65 bytes = "04" prefix + 64 bytes x + 64 bytes y)
        let pubkey = "0x04e68acfc0253a10620dff706b0a1b1f1f5833ea3beb3bde2250d5f271f3563606672ebc45e0b7ea2e816ecb70ca03137b1c9476eec63d4632e990020b7b6fba39";
        let addr = evm_address_from_public_key(pubkey).unwrap();
        assert_eq!(addr.as_bytes().len(), 20);
        // Verify against independently-computed expected address:
        // The function strips "0x" and the first byte ("04"), keccak256-hashes the
        // remaining 64-byte (x,y) coordinates, and takes the last 20 bytes.
        let key_bytes = hex::decode(
            "e68acfc0253a10620dff706b0a1b1f1f5833ea3beb3bde2250d5f271f3563606672ebc45e0b7ea2e816ecb70ca03137b1c9476eec63d4632e990020b7b6fba39"
        ).unwrap();
        let hash = keccak256(&key_bytes);
        let expected = H160::from_slice(&hash[12..]);
        assert_eq!(addr, expected);
    }

    #[test]
    fn evm_address_from_public_key_too_short() {
        let result = evm_address_from_public_key("0x04");
        assert!(result.is_err());
    }

    #[test]
    fn evm_address_from_public_key_deterministic() {
        let pubkey = "0x04e68acfc0253a10620dff706b0a1b1f1f5833ea3beb3bde2250d5f271f3563606672ebc45e0b7ea2e816ecb70ca03137b1c9476eec63d4632e990020b7b6fba39";
        let a1 = evm_address_from_public_key(pubkey).unwrap();
        let a2 = evm_address_from_public_key(pubkey).unwrap();
        assert_eq!(a1, a2);
    }

    // ── get_random_secret ──────────────────────────────────────────────
    #[test]
    fn get_random_secret_is_32_bytes() {
        let secret = get_random_secret();
        assert_eq!(secret.len(), 32);
    }

    #[test]
    fn get_random_secret_is_random() {
        let s1 = get_random_secret();
        let s2 = get_random_secret();
        assert_ne!(s1, s2);
    }
}
