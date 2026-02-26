use super::chain_info::Chain;
use super::models::{GetBalanceResponse, TransferRequest, TransferResponse};
use crate::core::api_status::ApiStatus;
use rocket::serde::json::Json;

pub async fn get_api_key_balance(
    _api_key: &str,
    chain: Chain,
) -> Result<GetBalanceResponse, ApiStatus> {
    let _ = (_api_key, chain);

    Ok(GetBalanceResponse {
        address: String::new(),
        balance: 0.0,
        chain: Chain::Ethereum,
        symbol: String::new(),
    })
}

pub async fn get_pkp_balance(
    pkp_public_key: &str,
    chain: Chain,
) -> Result<GetBalanceResponse, ApiStatus> {
    Ok(GetBalanceResponse {
        address: pkp_public_key.to_string(),
        balance: 0.0,
        chain,
        symbol: String::new(),
    })
}

pub async fn get_address_balance(
    address: &str,
    chain: Chain,
) -> Result<GetBalanceResponse, ApiStatus> {
    Ok(GetBalanceResponse {
        address: address.to_string(),
        balance: 0.0,
        chain,
        symbol: String::new(),
    })
}

pub async fn send(
    request: &Json<TransferRequest>,
    _chain: Chain,
) -> Result<TransferResponse, ApiStatus> {
    let _ = request;
    Ok(TransferResponse {
        txn_id: String::new(),
        success: false,
        chain: Chain::Ethereum,
        origin_symbol: String::new(),
        origin_amount: 0.0,
        gas: String::new(),
        timestamp: String::new(),
        destination_address: String::new(),
    })
}
