use ethers::utils::keccak256;

mod dstack;
pub mod endpoints;

/// Get the client key for a given derivation path.
pub async fn get_client_key(derivation_path: &str) -> Result<[u8; 32], String> {
    let path = format!("v1/{}", derivation_path);
    let purpose = "client";
    let key_response = dstack::get_key(path.as_str(), purpose)
        .await
        .map_err(|e| format!("failed to get key: {e}"))?;
    let secret = key_response
        .decode_key()
        .map_err(|e| format!("failed to decode key: {e}"))?;

    if secret.len() != 32 {
        return Err(format!("secret wrong length: {}", secret.len()));
    }

    // While this looks a bit redundant, it's necessary to ensure that multiple secrets can be exported
    // without compromising the security of the master key(s) used to derive them.
    let secret = keccak256(&secret);
    Ok(secret)
}
