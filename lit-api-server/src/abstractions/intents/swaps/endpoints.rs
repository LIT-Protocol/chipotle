//! Rocket endpoints for the swap intents API (v1).

use super::models::{
    AcceptQuoteRequest, AcceptQuoteResponse, GetOpenQuotesResponse, GetOpenSwapRequestsResponse,
    GetSwapStatusResponse, NewSwapRequest, NewSwapResponse, QuoteBalancesResponse,
    TokenListResponse,
};
use crate::abstractions::intents::swaps::internal;
use crate::abstractions::intents::swaps::models::{FillQuoteRequest, FillQuoteResponse};
use crate::core::models::ApiResult;
use crate::core::models::ErrMessage;
use rocket::serde::json::Json;
use rocket::{Route, get, post, routes};
use rocket_responder::ApiResponse;

pub fn routes() -> Vec<Route> {
    routes![
        get_contract_address,
        token_list,
        new_swap_request,
        fill_quote_request,
        accept_quote,
        get_swap_status,
        get_open_swap_requests,
        get_open_quotes,
        get_quote_balances,
        attempt_swap_request
    ]
}

/// GET /token_list — returns a list of supported tokens.
#[get("/token_list")]
async fn token_list() -> ApiResponse<TokenListResponse, ErrMessage> {
    ApiResult(internal::token_list().await).into()
}

/// POST /get_quote — get a quote for a swap.
#[post("/new_quote_request", format = "json", data = "<request>")]
async fn new_swap_request(
    request: Json<NewSwapRequest>,
) -> ApiResponse<NewSwapResponse, ErrMessage> {
    ApiResult(internal::new_swap_request(&request).await).into()
}

/// POST /fill_quote — fill a quote and get the transaction hash.
#[post("/fill_quote", format = "json", data = "<request>")]
async fn fill_quote_request(
    request: Json<FillQuoteRequest>,
) -> ApiResponse<FillQuoteResponse, ErrMessage> {
    ApiResult(internal::fill_quote_request(&request).await).into()
}

/// POST /accept_quote — accept a quote and get the PKP address to send funds to.
#[post("/accept_quote", format = "json", data = "<request>")]
async fn accept_quote(
    request: Json<AcceptQuoteRequest>,
) -> ApiResponse<AcceptQuoteResponse, ErrMessage> {
    ApiResult(internal::accept_quote(&request).await).into()
}

/// GET /get_swap_status/<quote_id> — get the status of a swap by quote id.
#[get("/get_swap_status/<quote_id>")]
async fn get_swap_status(quote_id: &str) -> ApiResponse<GetSwapStatusResponse, ErrMessage> {
    ApiResult(internal::get_swap_status(quote_id).await).into()
}

/// GET /get_open_swap_requests — returns open swap requests from the contract via getRecentSwapRequests.
#[get("/get_open_swap_requests")]
async fn get_open_swap_requests() -> ApiResponse<GetOpenSwapRequestsResponse, ErrMessage> {
    ApiResult(internal::get_open_swap_requests().await).into()
}

/// GET /get_open_quotes — returns open quotes from the contract via getRecentQuotes.
#[get("/get_open_quotes")]
async fn get_open_quotes() -> ApiResponse<GetOpenQuotesResponse, ErrMessage> {
    ApiResult(internal::get_open_quotes().await).into()
}

#[get("/get_contract_address")]
fn get_contract_address() -> ApiResponse<String, ErrMessage> {
    ApiResult(internal::get_contract_address()).into()
}

/// GET /get_quote_balances/<quote_id> — get quote data and PKP balance on source and destination chains.
#[get("/get_quote_balances/<quote_id>")]
async fn get_quote_balances(quote_id: &str) -> ApiResponse<QuoteBalancesResponse, ErrMessage> {
    ApiResult(internal::get_quote_balances(quote_id).await).into()
}

#[get("/attempt_swap_request/<quote_id>")]
async fn attempt_swap_request(quote_id: &str) -> ApiResponse<String, ErrMessage> {
    ApiResult(internal::attempt_swap_request(quote_id).await).into()
}
