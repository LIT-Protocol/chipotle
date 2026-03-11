use crate::{
    actions::client::models::SignedData, core::pkp_id_to_derviation_path,
    dstack::v1::get_client_key,
};
use anyhow::Result;
use k256::ecdsa::{
    Signature, SigningKey,
    signature::{Signer, hazmat::PrehashVerifier},
};
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

    if to_sign.len() != 32 {
        return Err(format!("To sign must be 32 bytes, got {}", to_sign.len()));
    }

    let signing_key = match SigningKey::from_slice(&secret_bytes) {
        Ok(signing_key) => signing_key,
        Err(e) => return Err(format!("Error creating signing key: {:?}", e)),
    };

    let verifying_key = signing_key.verifying_key();
    use sha3::{Digest, Keccak256};
    let mut hasher = Keccak256::new();
    hasher.update(to_sign);
    let hash = hasher.finalize(); // Result is a GenericArray<u8, 32>

    let signature: Signature = signing_key.sign(&hash);

    let r = verifying_key.verify_prehash(&hash, &signature);
    tracing::info!("Signature verification result: {:?}", r);

    let hex_signature = bytes_to_hex(signature.to_bytes());

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

// fn verify_keccak_signature(
//     public_key_bytes: &[u8],
//     original_data: &[u8],
//     signature: &Signature
// ) -> bool {
//     // 1. Re-calculate the Keccak256 hash of the original data
//     let mut hasher = Keccak256::new();
//     hasher.update(original_data);
//     let hash = hasher.finalize();

//     // 2. Initialize the VerifyingKey (Public Key)
//     // Works for both compressed (33 bytes) and uncompressed (65 bytes) formats
//     let verifying_key = VerifyingKey::from_sec1_bytes(public_key_bytes)
//         .expect("invalid public key");

//     // 3. Verify the signature against the pre-hashed value
//     // Returns Ok(()) if valid, or an Error if invalid
//     verifying_key.verify_prehash(&hash, signature).is_ok()
// }
