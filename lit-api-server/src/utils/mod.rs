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
    let derivation_path = bytes_to_hex(derivation_bytes);
    (derivation_u256, derivation_path)
}

pub fn evm_address(public_key: &str) -> Result<H160> {

    if public_key.len() < 32 {
        return Err(anyhow::anyhow!("Invalid public key length: {}", public_key.len()));
    }

    let pkp_address = hex::decode(&public_key.replace("0x", "")[2..])?;
    let pkp_address = keccak256(&pkp_address);    
    let pkp_address = H160::from_slice(&pkp_address[12..]);
    Ok(pkp_address)
}
