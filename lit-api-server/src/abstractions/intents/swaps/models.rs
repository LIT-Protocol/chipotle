//! Request and response models for the swap intents API.

use serde::{Deserialize, Serialize};

// ============== TokenList ==============

/// Request for `/v1/TokenList`. No parameters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenListRequest {}

/// A supported token in the token list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenListItem {
    /// Unique identifier (e.g. for updates when symbol changes).
    pub id: String,
    pub symbol: String,
    pub blockchain: String,
    /// USD equivalent price.
    pub usd_equivalent_price: String,
    /// When the USD price was set.
    pub usd_price_date: String,
}

/// Response for `/v1/TokenList`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenListResponse {
    pub tokens: Vec<TokenListItem>,
}

// ============== GetQuote ==============

/// How the quote is priced; determines where fees are taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum QuotePricingType {
    /// Destination amount stays the same; origin amount increases.
    Origin = 1,
    /// Origin sent stays the same; destination amount is reduced.
    Destination = 2,
}

/// Request for `/v1/GetQuote`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSwapRequest {
    /// Message sender (origin of the swap).
    pub from: String,
    pub origin_chain: String,
    pub origin_symbol: String,
    pub origin_amount: f32,
    pub destination_symbol: String,
    pub destination_chain: String,
    pub destination_amount: f32,
    /// Acceptable pricing slippage.
    pub slippage: f32,
    /// How the quote is priced (where fees are taken from).
    pub pricing_type: u8,
    /// How long to wait for a quote (0–60 seconds).
    pub quote_deadline_seconds: u16,
    /// Origin address (e.g. for cross-reference with the transaction).
    pub origin_address: String,
    /// Where to refund if the transaction does not go through.
    pub refund_address: String,
    /// Max time for the transaction to complete; after this, revert. Optional.
    pub transaction_deadline_seconds: u16,
    /// Optional message.
    pub message: Option<String>,
}

// ============== Contract-mirror types (quote.sol / QuoteStorage) ==============

/// Swap request as stored on-chain; mirrors `QuoteStorage.SwapRequest` in quote.sol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapRequestData {
    /// Message sender (origin of the swap).
    pub from: String,
    /// PKP address for this swap request.
    pub pkp_address: String,
    /// PKP token id (NFT token id) for this swap request.
    // pub pkp_token_id: String,
    pub origin_symbol: String,
    pub origin_chain: String,
    pub origin_amount: f64,
    pub destination_symbol: String,
    pub destination_chain: String,
    pub destination_amount: f64,
    /// Acceptable pricing slippage.
    pub slippage: u128,
    /// 0 = Origin, 1 = Destination (matches QuotePricingType enum in contract).
    pub pricing_type: u8,
    /// How long to wait for a quote (0–60 seconds).
    pub quote_deadline_seconds: u128,
    pub origin_address: String,
    pub refund_address: String,
    /// Max time for the transaction to complete; 0 if not set.
    pub transaction_deadline_seconds: u128,
    pub message: String,
}

/// Quote as stored on-chain; mirrors `QuoteStorage.Quote` in quote.sol.
/// The contract stores a mapping from quote id to this data; the linked swap request is referenced by id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteData {
    /// Id of the swap request this quote fulfils.
    pub swap_request_id: u128,
    pub provider_refund_address: String,
    /// Unix timestamp when the quote expires.
    pub quote_expiry: u64,
    /// Unix timestamp when the quote was created.
    pub created_at: u64,
    pub fees_total: u128,
    pub swap_request_data: SwapRequestData,
}

/// Response for GET open swap requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetOpenSwapRequestsResponse {
    pub swap_requests: Vec<SwapRequestData>,
}

/// Response for GET open quotes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetOpenQuotesResponse {
    pub quotes: Vec<QuoteData>,
}

/// Response for GET quote balances (PKP balance on source and destination chains).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteBalancesResponse {
    pub pkp_address: String,
    pub source_chain: String,
    pub destination_chain: String,
    /// Balance in wei (native token) as string.
    pub source_balance_wei: String,
    /// Balance in wei (native token) as string.
    pub destination_balance_wei: String,

    pub source_balance_sufficient: bool,
    pub destination_balance_sufficient: bool,
}

/// A single fee component in a quote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteFee {
    /// Who receives the fee (e.g. protocol, relay).
    pub recipient: Option<String>,
    pub amount: String,
    pub symbol: Option<String>,
}

/// Response for `/v1/GetQuote`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSwapResponse {
    pub swap_request_id: String,
    /// When the quote expires.
    pub swap_request_expiry: String,
    /// List of fee components.
    pub fees: Vec<QuoteFee>,
    /// Hash of input parameters signed by LIT (e.g. with network key).
    pub signed_input_hash: String,
    pub transaction_hash: String,
}

// ============== FillQuote ==============

/// Request for `/v1/FillQuote`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillQuoteRequest {
    pub swap_request_id: String,
    pub quote_deadline_seconds: u32,
    pub provider_refund_address: String,
    pub message: Option<String>,
}

/// Response for `/v1/FillQuote`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillQuoteResponse {
    pub quote_id: String,
    pub transaction_hash: String,
    pub pkp_address: String,
    pub swap_request_id: String,
    pub quote_expiry: String,
    pub fees: Vec<QuoteFee>,
    pub total_fees: u128,
    pub signed_input_hash: String,
}

// ============== AcceptQuote ==============

/// Request for `/v1/AcceptQuote`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptQuoteRequest {
    pub quote_id: String,
}

/// Response for `/v1/AcceptQuote`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptQuoteResponse {
    /// PKP address to send the funds to.
    pub pkp_address: String,
}

// ============== GetSwapStatus ==============

/// Request for `/v1/GetSwapStatus`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetSwapStatusRequest {
    pub quote_id: String,
}

/// State of a swap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum SwapState {
    Pending,
    Processing,
    Success,
    Expired,
    Refunded,
    /// Catch-all for other states (e.g. Failed, Cancelled).
    Other,
}

/// Swap and quote details returned with status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapQuoteDetails {
    pub quote_id: String,
    pub origin_symbol: Option<String>,
    pub origin_amount: Option<String>,
    pub destination_symbol: Option<String>,
    pub destination_amount: Option<String>,
    pub pkp_address: Option<String>,
    /// Additional status-specific fields as needed.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Response for `/v1/GetSwapStatus`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetSwapStatusResponse {
    pub state: SwapState,
    /// Swap and quote details when available.
    pub details: Option<SwapQuoteDetails>,
}
