use super::utils::pkp_id_to_derviation_path;
use crate::{actions::client::models::SignedData, dstack::v1::get_client_key, utils::{evm_address, parse_to_hash::string_to_h160}};
use anyhow::Result;
use elliptic_curve::group::GroupEncoding;
use k256::ecdsa::SigningKey;
use lit_core::utils::binary::{bytes_to_0x_hex, bytes_to_hex};

pub async fn sign_with_pkp(
    api_key: &str,
    pkp_id: &str,
    to_sign: &[u8],
    sig_name: &str,
    signing_scheme: &str,
) -> Result<(String, SignedData), String> {
    let derivation_path = pkp_id_to_derviation_path(api_key, pkp_id).await?;
    let secret_bytes = get_client_key(&derivation_path).await?;


    if to_sign.len() != 32 {
        return Err(format!("To sign must be 32 bytes, got {}", to_sign.len()));
    }
    
    let signing_key = match SigningKey::from_slice(&secret_bytes) {
        Ok(signing_key) => signing_key,
        Err(e) => return Err(format!("Error creating signing key: {:?}", e)),
    };

    let verifying_key = signing_key.verifying_key();
    let public_key_bytes = verifying_key.as_affine().to_bytes();
    let public_key_string = bytes_to_0x_hex(&public_key_bytes);

    let pkp_id_address = string_to_h160(pkp_id).map_err(|e| format!("Error converting PKP ID to EVM address: {:?}", e))?;
    let evm_address = evm_address(&public_key_string).map_err(|e| format!("Error converting public key to EVM address: {:?}", e))?;

    if pkp_id_address != evm_address {
        return Err(format!("PKP ID does not match public key: {:?} != {:?}", pkp_id_address, evm_address));
    }
    
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
