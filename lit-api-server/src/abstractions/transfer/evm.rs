use super::chain_info::Chain;
use super::models::{GetBalanceResponse, TransferRequest, TransferResponse};
use crate::core::api_status::ApiStatus;
use ethers::providers::{Http, Middleware, Provider};
use ethers::types::H160;
use ethers::utils::{format_ether, keccak256};
use lit_core::utils::binary::{bytes_to_hex, hex_to_bytes};
use rocket::serde::json::Json;
use tracing::info;

fn not_configured() -> ApiStatus {
    ApiStatus::internal_server_error(
        anyhow::anyhow!("Lit testnet not configured"),
        "This operation is not available.",
    )
}

pub async fn get_api_key_balance(
    api_key: &str,
    chain: Chain,
) -> Result<GetBalanceResponse, ApiStatus> {
    let _ = (api_key, chain);
    Err(not_configured())
}

pub async fn get_pkp_balance(
    pkp_public_key: &str,
    chain: Chain,
) -> Result<GetBalanceResponse, ApiStatus> {
    let pkp_address = evm_address(pkp_public_key).await?;
    get_balance(pkp_address, chain).await
}

pub async fn get_address_balance(
    address: &str,
    chain: Chain,
) -> Result<GetBalanceResponse, ApiStatus> {
    let address = H160::from_slice(hex_to_bytes(address)?.as_slice());
    get_balance(address, chain).await
}

async fn get_balance(address: H160, chain: Chain) -> Result<GetBalanceResponse, ApiStatus> {
    let provider = Provider::<Http>::try_from(chain.info().rpc_url)?;
    info!("provider: {:?}", provider);

    let block = None;
    let balance = provider.get_balance(address, block).await?;

    let balance = format_ether(balance).parse::<f64>()?;

    Ok(GetBalanceResponse {
        address: bytes_to_hex(&address.as_bytes()),
        balance: balance,
        chain: chain.clone(),
        symbol: chain.info().token.to_string(),
    })
}

async fn evm_address(public_key: &str) -> Result<H160, ApiStatus> {
    let pkp_address = hex::decode(&public_key.replace("0x", "")[2..])?;
    let pkp_address = keccak256(&pkp_address);
    let pkp_address = H160::from_slice(&pkp_address[12..]);

    Ok(pkp_address)
}

pub async fn send(
    request: &Json<TransferRequest>,
    chain: Chain,
) -> Result<TransferResponse, ApiStatus> {
    let _ = (request, chain);
    Err(not_configured())
}
