use crate::utils::{parse_with_hash::pkp_id_to_h160, u256_to_derviation_path};

pub mod account_management;
pub mod core_features;
pub mod v1;
pub mod lookup_data;

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
