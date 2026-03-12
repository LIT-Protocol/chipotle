use ethers::utils::keccak256;

use crate::utils::generate_lit_action_derivation_path;
mod dstack;
pub mod endpoints;

/// Get the client key for a given derivation path.
pub async fn get_client_key(derivation_path: &str) -> Result<[u8; 32], String> {
    let path = format!("v1/{}", derivation_path);
    let purpose = "client";
    get_key(path.as_str(), purpose).await
}

pub async fn get_lit_action_key(ipfs_id: &str) -> Result<[u8; 32], String> {
    let ipfs_with_prefix = format!("lit_action_{}", ipfs_id);
    let derivation_path = generate_lit_action_derivation_path(ipfs_with_prefix.as_str());
    let path = format!("v1/{}", derivation_path);
    let purpose = "lit_action";
    get_key(path.as_str(), purpose).await
}

/// Get the client key for a given payer number.
pub async fn get_lit_payer_key(payer_number: u16) -> Result<[u8; 32], String> {
    let path = format!("v1/payer_{}", payer_number);
    let purpose = "lit_payer";
    get_key(path.as_str(), purpose).await
}

pub async fn get_admin_api_payer_key() -> Result<[u8; 32], String> {
    let path = "v1/admin_api_payer".to_string();
    let purpose = "lit_payer";
    get_key(path.as_str(), purpose).await
}

async fn get_key(path: &str, purpose: &str) -> Result<[u8; 32], String> {
    let key_response = dstack::get_key(path, purpose)
        .await
        .map_err(|e| format!("failed to get key: {e}"))?;
    let secret = key_response
        .decode_key()
        .map_err(|e| format!("failed to decode key: {e}"))?;

    if secret.len() != 32 {
        return Err(format!("secret wrong length: {}", secret.len()));
    }

    // While this looks a bit redundant, it's necessary to ensure that more than 1 secret can be exported
    // without compromising the security of the master key(s) used to derive them.
    let secret = keccak256(&secret);
    Ok(secret)
}
