pub mod chain_info;
pub mod parse_to_hash;
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
