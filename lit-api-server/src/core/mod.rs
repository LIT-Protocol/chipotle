use crate::dstack::v1::get_client_key;
use crate::utils::{parse_with_hash::pkp_id_to_h160, u256_to_derviation_path};
use alloy::signers::local::PrivateKeySigner;

pub mod account_management;
pub mod cache_metadata;
pub mod core_features;
pub mod eip712;
pub mod v1;

pub async fn pkp_id_to_derviation_path(api_key: &str, pkp_id: &str) -> Result<String, String> {
    let wallet_address = pkp_id_to_h160(pkp_id)
        .map_err(|e| format!("Error converting PKP ID to EVM address: {:?}", e))?;
    let derivation_u256 =
        match crate::accounts::get_wallet_derivation(api_key, wallet_address).await {
            Ok(secret_u256) => secret_u256,
            Err(e) => return Err(format!("Error getting wallet derivation: {:?}", e)),
        };

    let derivation_path = u256_to_derviation_path(derivation_u256);
    Ok(derivation_path)
}

/// Resolve the derivation path for `pkp_id`, derive the client key, and verify
/// the key actually belongs to that pkpId before returning it. This is the single
/// chokepoint every PKP key-release op (get_private_key, aes_encrypt/decrypt)
/// goes through.
///
/// A PKP's pkpId (wallet address) is, by construction,
/// `address(secp256k1(get_client_key(path)))` — see
/// `account_management::create_new_wallet`. The on-chain registry does NOT
/// enforce that invariant: `registerWalletDerivation` stores whatever
/// `(pkpId, derivationPath)` pair the caller supplies, so a caller can register a
/// fresh, self-owned pkpId aliased to a *victim's* public derivation path and
/// otherwise drive the node into releasing the victim's key. This is the
/// last-line cryptographic check at the key-release boundary: derive the key,
/// recompute its address, and refuse to release it if that address is not the
/// requested pkpId. Because it verifies the cryptographic identity directly
/// (not an on-chain ownership record), it closes cross-account key theft for
/// every wallet — including any registered before the on-chain path-owner
/// binding existed, with no migration required.
pub async fn get_verified_client_key(api_key: &str, pkp_id: &str) -> Result<[u8; 32], String> {
    let expected_address = pkp_id_to_h160(pkp_id)
        .map_err(|e| format!("Error converting PKP ID to EVM address: {:?}", e))?;
    let derivation_path = pkp_id_to_derviation_path(api_key, pkp_id).await?;
    let secret = get_client_key(&derivation_path).await?;

    let signer = PrivateKeySigner::from_slice(&secret)
        .map_err(|e| format!("Error deriving signer from client key: {:?}", e))?;
    if signer.address() != expected_address {
        // The stored path derives a key for a different wallet than the pkpId the
        // caller asked for — i.e. the (pkpId, path) registration is an alias for
        // someone else's key. Fail closed instead of releasing it.
        return Err("derivation path does not match pkpId".to_string());
    }

    Ok(secret)
}
