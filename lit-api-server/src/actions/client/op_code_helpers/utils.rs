use anyhow::Result;
use ethers::types::H160;
use lit_core::utils::binary::{bytes_to_hex, hex_to_bytes};

pub async fn pkp_id_to_derviation_path(api_key: &str, pkp_id: &str) -> Result<String, String> {
    let src =
        &hex_to_bytes(pkp_id).map_err(|e| format!("Error converting PKP ID to bytes: {:?}", e))?;
    if src.len() != 20 {
        return Err(format!("Invalid PKP ID length: {} (expected 20): Ppk Id {}", src.len(), pkp_id));
    }
    let wallet_address = H160::from_slice(src);
    let derivation_u256 =
        match crate::accounts::get_wallet_derivation(api_key, wallet_address).await {
            Ok(secret_u256) => secret_u256,
            Err(e) => return Err(format!("Error getting wallet derivation: {:?}", e)),
        };

    if derivation_u256 == ethers::types::U256::zero() {
        return Err("Wallet not found".to_string());
    }

    let mut derivation_bytes = [0; 32];
    derivation_u256.to_big_endian(&mut derivation_bytes);

    let derivation_path = bytes_to_hex(derivation_bytes);
    Ok(derivation_path)
}
