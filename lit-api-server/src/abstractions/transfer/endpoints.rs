//! Rocket endpoints for transfer (balance and send).

use crate::abstractions::transfer::models::ChainInfoResponse;
use crate::abstractions::transfer::{btc, evm, non_evm};

use super::chain_info::Chain;
use super::models::{GetBalanceResponse, GetChainsResponse, TransferRequest, TransferResponse};
use crate::core::api_status::ErrMessage;
use crate::core::api_status::{ApiResult, ApiStatus};
use rocket::serde::json::Json;
use rocket::{Route, get, post, routes};
use rocket_responder::{ApiResponse, internal_server_error, ok};
use tracing::{error, info};

pub fn routes() -> Vec<Route> {
    routes![
        get_api_key_balance,
        get_pkp_balance,
        get_address_balance,
        get_all_chains,
        send
    ]
}
/// GET /get_balance/<api_key>/<chain> — get balance for an address on a chain.
#[get("/get_api_key_balance/<api_key>/<chain>")]
async fn get_api_key_balance(
    api_key: &str,
    chain: &str,
) -> ApiResponse<GetBalanceResponse, ErrMessage> {
    let chain = match Chain::try_from_str(chain) {
        Ok(chain) => chain,
        Err(e) => {
            error!("Invalid chain: {:?}", e);
            return internal_server_error(ErrMessage(format!("Invalid chain: {:?}", e)));
        }
    };

    match chain.info().is_evm {
        true => ApiResult(evm::get_api_key_balance(api_key, chain).await).into(),
        false => ApiResult(btc::get_api_key_balance(api_key, chain).await).into(),
    }
}

#[get("/get_pkp_balance/<pkp_public_key>/<chain>")]
async fn get_pkp_balance(
    pkp_public_key: &str,
    chain: &str,
) -> ApiResponse<GetBalanceResponse, ErrMessage> {
    info!("get_pkp_balance: {:?}, {:?}", pkp_public_key, chain);

    let chain = match Chain::try_from_str(chain) {
        Ok(chain) => chain,
        Err(e) => {
            error!("Invalid chain: {:?}", e);
            return internal_server_error(ErrMessage(format!("Invalid chain: {:?}", e)));
        }
    };

    match chain.info().is_evm {
        true => ApiResult(evm::get_pkp_balance(pkp_public_key, chain).await).into(),
        false => ApiResult(non_evm::get_pkp_balance(pkp_public_key, chain).await).into(),
    }
}

#[get("/get_address_balance/<address>/<chain>")]
async fn get_address_balance(
    address: &str,
    chain: &str,
) -> ApiResponse<GetBalanceResponse, ErrMessage> {
    let chain = match Chain::try_from_str(chain) {
        Ok(chain) => chain,
        Err(e) => {
            error!("Invalid chain: {:?}", e);
            return internal_server_error(ErrMessage(format!("Invalid chain: {:?}", e)));
        }
    };

    match chain.info().is_evm {
        true => ApiResult(evm::get_address_balance(address, chain).await).into(),
        false => ApiResult(non_evm::get_address_balance(address, chain).await).into(),
    }
}

/// GET /get_chains?is_evm=true&is_testnet=false — query params (GET with body is unreliable in browsers).
#[get("/get_chains?<is_evm>&<is_testnet>")]
async fn get_all_chains(
    is_evm: Option<&str>,
    is_testnet: Option<&str>,
) -> ApiResponse<GetChainsResponse, ErrMessage> {
    let is_evm = is_evm
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(true);
    let is_testnet = is_testnet
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let chains = if is_evm {
        if is_testnet {
            Chain::all_testnet_evm_chains()
        } else {
            Chain::all_evm_chains()
        }
    } else {
        Chain::all_non_evm_chains()
    };

    info!("chains: {:?}", chains);

    ok(GetChainsResponse {
        chains: chains
            .iter()
            .map(|chain| ChainInfoResponse {
                chain: chain.info().chain.to_string(),
                display_name: chain.info().chain_name.to_string(),
                token: chain.info().token.to_string(),
            })
            .collect(),
    })
}

/// POST /send — send funds to a destination address on a chain.
#[post("/send", format = "json", data = "<request>")]
async fn send(request: Json<TransferRequest>) -> ApiResponse<TransferResponse, ErrMessage> {
    ApiResult(internal_send(&request).await).into()
}

pub async fn internal_send(request: &Json<TransferRequest>) -> Result<TransferResponse, ApiStatus> {
    info!("request: {:?}", request);

    let chain = match Chain::try_from_str(request.chain.as_str()) {
        Ok(chain) => chain,
        Err(s) => return Err(ApiStatus::from(s)),
    };

    match chain.info().is_evm {
        true => evm::send(&request, chain).await,
        false => non_evm::send(&request, chain).await,
    }
}
