//! Rocket endpoints for the swap intents API (v1).

use super::models::{
    AcceptQuoteRequest, AcceptQuoteResponse, GetOpenQuotesResponse, GetOpenSwapRequestsResponse,
    GetSwapStatusResponse, NewSwapRequest, NewSwapResponse, QuoteBalancesResponse, QuoteData,
    SwapRequestData, SwapState, TokenListResponse,
};
use crate::abstractions::intents::swaps::QUOTE_STORAGE_ADDRESS;
use crate::abstractions::intents::swaps::contracts::quote_storage::{
    Quote, QuoteStorage, SwapRequest,
};
use crate::abstractions::intents::swaps::models::{FillQuoteRequest, FillQuoteResponse};
use crate::abstractions::transfer::chain_info::Chain;
use crate::core::v1::helpers::api_status::ApiStatus;
use anyhow::Result;
use ethers::middleware::SignerMiddleware;
use ethers::providers::Http;
use ethers::providers::Middleware;
use ethers::providers::Provider;
use ethers::signers::{LocalWallet, Signer};
use ethers::types::{H160, U256};
use ethers::utils::format_ether;
use lit_core::utils::binary::{bytes_to_hex, hex_to_bytes};
use std::sync::Arc;
use tracing::info;

fn not_configured() -> ApiStatus {
    ApiStatus::internal_server_error(
        anyhow::anyhow!("Lit testnet not configured"),
        "This operation is not available.",
    )
}

/// GET /token_list — returns a list of supported tokens.
pub async fn token_list() -> Result<TokenListResponse, ApiStatus> {
    Ok(TokenListResponse { tokens: vec![] })
}

/// POST /get_quote — get a quote for a swap.
pub async fn new_swap_request(request: &NewSwapRequest) -> Result<NewSwapResponse, ApiStatus> {
    let _ = request;
    Err(not_configured())
}

/// POST /fill_quote — fill a quote and get the transaction hash.
pub async fn fill_quote_request(
    request: &FillQuoteRequest,
) -> Result<FillQuoteResponse, ApiStatus> {
    let swap_request_id_str = request.swap_request_id.clone();
    let swap_request_id = request.swap_request_id.clone();
    let swap_request_id = match U256::from_dec_str(swap_request_id.as_str()) {
        Ok(id) => id,
        Err(e) => return Err(ApiStatus::bad_request(e.into(), "Invalid swap request id.")),
    };

    let provider_refund_address = hex_to_bytes(request.provider_refund_address.as_str())?;
    let provider_refund_address = H160::from_slice(&provider_refund_address);

    let contract = get_signable_quote_contract().await?;
    let func = contract.new_quote(swap_request_id, provider_refund_address);
    let tx = func
        .send()
        .await
        .map_err(|e| ApiStatus::internal_server_error(e.into(), "Failed to enternew quote."))?;
    let quote_id = bytes_to_hex(tx.0);
    let receipt = tx.await?;
    let transaction_hash = receipt
        .ok_or(ApiStatus::option_not_found("Transaction receipt missing"))?
        .transaction_hash;

    let duration = std::time::Duration::from_secs(request.quote_deadline_seconds.into());

    let pkp_address =
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string();

    Ok(FillQuoteResponse {
        quote_id: quote_id.to_string(),
        transaction_hash: transaction_hash.to_string(),
        pkp_address: pkp_address.to_string(),
        swap_request_id: swap_request_id_str,
        quote_expiry: format!(
            "{:?}",
            std::time::Instant::now()
                .checked_add(duration)
                .ok_or(ApiStatus::option_not_found("Duration is None"))?
        ),
        fees: vec![],
        signed_input_hash: String::new(),
        total_fees: 0,
    })
}

/// POST /accept_quote — accept a quote and get the PKP address to send funds to.
pub async fn accept_quote(request: &AcceptQuoteRequest) -> Result<AcceptQuoteResponse, ApiStatus> {
    let _ = request;
    Ok(AcceptQuoteResponse {
        pkp_address: String::new(),
    })
}

/// GET /get_swap_status/<quote_id> — get the status of a swap by quote id.
pub async fn get_swap_status(quote_id: &str) -> Result<GetSwapStatusResponse, ApiStatus> {
    let _ = quote_id;
    Ok(GetSwapStatusResponse {
        state: SwapState::Pending,
        details: None,
    })
}

/// GET /get_open_swap_requests — returns open swap requests from the contract via getRecentSwapRequests.
pub async fn get_open_swap_requests() -> Result<GetOpenSwapRequestsResponse, ApiStatus> {
    let contract = get_signable_quote_contract()
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "Failed to get quoting contract."))?;

    let count = contract
        .open_swap_requests_count()
        .call()
        .await
        .map_err(|e| {
            ApiStatus::internal_server_error(e.into(), "Failed to get open swap requests count.")
        })?;

    if count.is_zero() {
        return Ok(GetOpenSwapRequestsResponse {
            swap_requests: vec![],
        });
    }

    let list: Vec<SwapRequest> = contract
        .get_recent_swap_requests(count)
        .call()
        .await
        .map_err(|e| {
            ApiStatus::internal_server_error(e.into(), "Failed to get recent swap requests.")
        })?;

    let swap_requests = list
        .iter()
        .map(swap_request_to_data)
        .collect::<Result<Vec<SwapRequestData>, ApiStatus>>()?;
    Ok(GetOpenSwapRequestsResponse { swap_requests })
}

/// GET /get_open_quotes — returns open quotes from the contract via getRecentQuotes.
pub async fn get_open_quotes() -> Result<GetOpenQuotesResponse, ApiStatus> {
    let contract = get_signable_quote_contract()
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "Failed to get quoting contract."))?;
    let count: U256 = contract.open_quotes_count().call().await.map_err(|e| {
        ApiStatus::internal_server_error(e.into(), "Failed to get open quotes count.")
    })?;
    if count.is_zero() {
        return Ok(GetOpenQuotesResponse { quotes: vec![] });
    }
    let list: Vec<Quote> = contract
        .get_recent_quotes(count)
        .call()
        .await
        .map_err(|e| ApiStatus::internal_server_error(e.into(), "Failed to get recent quotes."))?;

    let swap_request_ids: Vec<U256> = list.iter().map(|q| q.swap_request_id).collect();
    info!("swap request ids: {:?}", swap_request_ids);
    let swap_requests: Vec<SwapRequest> = contract
        .get_requests_by_ids(swap_request_ids)
        .call()
        .await
        .map_err(|e| {
            ApiStatus::internal_server_error(e.into(), "Failed to get requests by ids.")
        })?;

    info!("swap_requests: {:?}", swap_requests);

    let quotes = list
        .iter()
        .zip(swap_requests.iter())
        .map(|(q, sr)| quote_to_data(q, sr).unwrap())
        .collect();
    info!("quotes: {:?}", quotes);
    Ok(GetOpenQuotesResponse { quotes })
}

pub fn get_contract_address() -> Result<String, ApiStatus> {
    Ok(QUOTE_STORAGE_ADDRESS.to_string())
}

/// GET /get_quote_balances/<quote_id> — get quote data and PKP balance on source and destination chains.
pub async fn get_quote_balances(quote_id: &str) -> Result<QuoteBalancesResponse, ApiStatus> {
    let quote_id_u256 = U256::from_dec_str(quote_id)
        .map_err(|e| ApiStatus::bad_request(e.into(), "Invalid quote_id."))?;

    let contract = get_signable_quote_contract()
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "Failed to get quoting contract."))?;
    let quote = contract
        .get_quote(quote_id_u256)
        .call()
        .await
        .map_err(|e| ApiStatus::internal_server_error(e.into(), "Failed to get quote."))?;

    let swap_request: SwapRequest = contract
        .get_swap_request(quote.swap_request_id)
        .call()
        .await
        .map_err(|e| ApiStatus::internal_server_error(e.into(), "Failed to get swap request."))?;

    let src_chain = Chain::try_from_str(swap_request.origin_chain.as_str())
        .map_err(|e| ApiStatus::from(e).add_message("Unsupported origin chain."))?;
    let dst_chain = Chain::try_from_str(swap_request.destination_chain.as_str())
        .map_err(|e| ApiStatus::from(e).add_message("Unsupported destination chain."))?;

    let pkp_address = swap_request.pkp_address;
    let src_provider = Provider::<Http>::try_from(src_chain.rpc_url().as_str()).map_err(|e| {
        ApiStatus::internal_server_error(e.into(), "Failed to create source chain provider.")
    })?;
    let dst_provider = Provider::<Http>::try_from(dst_chain.rpc_url().as_str()).map_err(|e| {
        ApiStatus::internal_server_error(e.into(), "Failed to create destination chain provider.")
    })?;

    let src_balance = src_provider
        .get_balance(pkp_address, None)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e.into(), "get_balance (source) failed."))?;
    let dst_balance = dst_provider
        .get_balance(pkp_address, None)
        .await
        .map_err(|e| {
            ApiStatus::internal_server_error(e.into(), "Failed to get balance (destination).")
        })?;

    info!(
        "src_balance: {:?}, dst_balance: {:?}",
        src_balance, dst_balance
    );
    info!(
        "swap_request.origin_amount: {:?}, swap_request.destination_amount: {:?}",
        swap_request.origin_amount, swap_request.destination_amount
    );

    Ok(QuoteBalancesResponse {
        pkp_address: format!("{:?}", pkp_address),
        source_chain: swap_request.origin_chain.clone(),
        destination_chain: swap_request.destination_chain.clone(),
        source_balance_wei: src_balance.to_string(),
        destination_balance_wei: dst_balance.to_string(),
        source_balance_sufficient: src_balance >= swap_request.origin_amount,
        destination_balance_sufficient: dst_balance >= swap_request.destination_amount,
    })
}

pub async fn attempt_swap_request(quote_id: &str) -> Result<String, ApiStatus> {
    let _ = quote_id;
    Err(not_configured())
}

fn swap_request_to_data(sr: &SwapRequest) -> Result<SwapRequestData, ApiStatus> {
    Ok(SwapRequestData {
        from: format!("{:?}", sr.from),
        pkp_address: format!("{:?}", sr.pkp_address),
        origin_symbol: sr.origin_symbol.clone(),
        origin_chain: sr.origin_chain.clone(),
        origin_amount: format_ether(sr.origin_amount).parse::<f64>()?,
        destination_symbol: sr.destination_symbol.clone(),
        destination_chain: sr.destination_chain.clone(),
        destination_amount: format_ether(sr.destination_amount).parse::<f64>()?,
        slippage: sr.slippage.as_u128(),
        pricing_type: sr.pricing_type,
        quote_deadline_seconds: sr.quote_deadline_seconds.as_u128(),
        origin_address: format!("{:?}", sr.origin_address),
        refund_address: format!("{:?}", sr.refund_address),
        transaction_deadline_seconds: sr.transaction_deadline_seconds.as_u128(),
        message: sr.message.clone(),
    })
}

fn quote_to_data(q: &Quote, sr: &SwapRequest) -> Result<QuoteData, ApiStatus> {
    Ok(QuoteData {
        swap_request_id: q.swap_request_id.as_u128(),
        provider_refund_address: format!("{:?}", q.provider_refund_address),
        quote_expiry: q.quote_expiry.as_u64(),
        created_at: q.created_at.as_u64(),
        fees_total: q.fees_total.as_u128(),
        swap_request_data: swap_request_to_data(sr)?,
    })
}

async fn get_signable_quote_contract()
-> Result<QuoteStorage<SignerMiddleware<Provider<Http>, LocalWallet>>, anyhow::Error> {
    let chain = Chain::Yellowstone;
    let chain_info = chain.info();
    let secret = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    let secret = hex_to_bytes(secret)?;

    let wallet = LocalWallet::from_bytes(&secret)?.with_chain_id(chain_info.chain_id);

    let provider = Provider::<Http>::try_from(chain.rpc_url().as_str())?;
    let signing_provider = SignerMiddleware::new(provider.clone(), wallet);

    let client = Arc::new(signing_provider);
    let quote_storage_address = hex_to_bytes(QUOTE_STORAGE_ADDRESS)?;
    let quote_storage_address = H160::from_slice(&quote_storage_address);
    let contract = QuoteStorage::new(quote_storage_address, client);
    Ok(contract)
}
