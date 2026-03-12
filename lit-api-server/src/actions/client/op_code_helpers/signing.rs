use super::utils::pkp_id_to_derviation_path;
use crate::{actions::client::models::SignedData, dstack::v1::get_client_key};
use anyhow::Result;
use k256::ecdsa::SigningKey;
use lit_core::utils::binary::bytes_to_hex;

pub async fn sign_with_pkp(
    api_key: &str,
    pkp_id: &str,
    to_sign: &[u8],
    sig_name: &str,
    signing_scheme: &str,
) -> Result<(String, SignedData), String> {
    let derivation_path = pkp_id_to_derviation_path(api_key, pkp_id).await?;
    let secret_bytes = get_client_key(&derivation_path).await?;

    let signing_key = match SigningKey::from_slice(&secret_bytes) {
        Ok(signing_key) => signing_key,
        Err(e) => return Err(format!("Error creating signing key: {:?}", e)),
    };

    let signature = match signing_key.sign_recoverable(to_sign) {
        Ok(signature) => signature,
        Err(e) => return Err(format!("Error signing: {:?}", e)),
    };
    let hex_signature = bytes_to_hex(signature.0.to_vec());

    Ok((
        sig_name.to_string(),
        SignedData {
            signing_scheme: signing_scheme.to_string(),
            digest: bytes_to_hex(to_sign),
            pkp_id: pkp_id.to_string(),
            signature: hex_signature.clone(),
        },
    ))
}
