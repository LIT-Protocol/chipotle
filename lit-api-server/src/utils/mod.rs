pub mod parse_to_hash;

use ethers::types::U256;
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
