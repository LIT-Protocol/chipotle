use super::chain_info::Chain;
use super::models::{GetBalanceResponse, TransferRequest, TransferResponse};
use crate::core::models::ApiStatus;
use rocket::serde::json::Json;

const TPRV: &str = "prv8ZgxMBicQKsPfEr2kLSUC5JG5UcFQvgFpYd3ntCUk4QUb35DdZTpQZmHPcMdD9kERXjCoT5Pw7WEJwNxRD1o6ZecamNFokodcnoPRHpaRS4";
const SECRET_KEY: &str = "WEJwNxRD1o6ZecamNFokodcnoPRHpaRS4";

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
    Ok(GetBalanceResponse {
        address: pkp_public_key.to_string(),
        balance: 0.0,
        chain: chain.clone(),
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
        chain: chain.clone(),
        symbol: String::new(),
    })
}

// async fn get_balance(address: H160, chain: Chain) -> Result<GetBalanceResponse, ApiStatus> {
//     // let provider = Provider::<Http>::try_from(chain.info().rpc_url)?;
//     // info!("provider: {:?}", provider);

//     // let block = None;
//     // let balance = provider.get_balance(address, block).await?;

//     // let balance = format_ether(balance).parse::<f64>()?;

//     // Ok(GetBalanceResponse {
//     //     address: bytes_to_hex(&address.as_bytes()),
//     //     balance: balance,
//     //     chain: chain.clone(),
//     //     symbol: chain.info().token.to_string(),
//     // })

//       Ok(GetBalanceResponse {
//         address: "".to_string(),
//         balance: 0.0,
//         chain: chain.clone(),
//         symbol: chain.info().token.to_string(),
//       })

// }

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
