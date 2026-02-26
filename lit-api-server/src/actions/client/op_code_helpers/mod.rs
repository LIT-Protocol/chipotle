use crate::actions::client::models::SignedData;
use anyhow::Result;
use lit_core::utils::binary::bytes_to_hex;
use lit_rust_crypto::k256::ecdsa::SigningKey;

pub async fn sign_with_pkp(
    api_key: &str,
    public_key: &str,
    to_sign: &[u8],
    sig_name: &str,
    signing_scheme: &str,
) -> Result<(String, SignedData), String> {
    let secret_u256 =
        match crate::accounts::get_wallet_derivation_from_pubkey(api_key, public_key).await {
            Ok(secret_u256) => secret_u256,
            Err(e) => return Err(format!("Error getting wallet derivation: {:?}", e)),
        };

    if secret_u256 == ethers::types::U256::zero() {
        return Err("Wallet not found".to_string());
    }

    let mut secret_bytes = [0; 32];
    secret_u256.to_big_endian(&mut secret_bytes);

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
            public_key: public_key.to_string(),
            signature: hex_signature.clone(),
        },
    ))
}

pub async fn decrypt_with_pkp() {}
