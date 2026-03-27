/**
 * Lit Node Simple API - JavaScript Swaps SDK
 *
 * Wrapper for swap intents API endpoints in src/abstractions/intents/swaps/endpoints.rs.
 * Mount the swaps routes at /swaps/v1/ (e.g. in main.rs: .mount("/swaps/v1/", ...)).
 *
 * Endpoints (from endpoints.rs):
 *   GET  /get_contract_address - Quote storage contract address
 *   GET  /token_list
 *   POST /new_quote_request  (NewSwapRequest -> NewSwapResponse)
 *   POST /fill_quote         (FillQuoteRequest -> FillQuoteResponse)
 *   POST /accept_quote       (AcceptQuoteRequest -> AcceptQuoteResponse)
 *   GET  /get_swap_status/<quote_id>
 *   GET  /get_open_swap_requests  (contract: getRecentSwapRequests)
 *   GET  /get_open_quotes        (contract: getRecentQuotes)
 *   GET  /get_quote_balances/<quote_id> - Quote data + PKP balance on source and destination chains
 *   GET  /attempt_swap_request/<quote_id> - Attempt to execute the swap for a quote
 */

// --- Token list ---

/**
 * @typedef {Object} TokenListItem
 * Matches TokenListItem in models.rs.
 * @property {string} id - Unique identifier
 * @property {string} symbol - Token symbol
 * @property {string} blockchain - Blockchain identifier
 * @property {string} usd_equivalent_price - USD equivalent price
 * @property {string} usd_price_date - When the USD price was set
 */

/**
 * @typedef {Object} TokenListResponse
 * @property {TokenListItem[]} tokens - List of supported tokens
 */

// --- New quote request ---

/** Quote pricing type: fees from origin amount. Matches contract enum 0 (Origin). */
export const QUOTE_PRICING_ORIGIN = 0;
/** Quote pricing type: fees from destination amount. Matches contract enum 1 (Destination). */
export const QUOTE_PRICING_DESTINATION = 1;

/**
 * @typedef {Object} NewSwapRequestOptions
 * @property {string} from - Message sender (origin of the swap), hex address
 * @property {string} originChain - Origin chain key (e.g. "ethereum", "yellowstone")
 * @property {string} originSymbol - Origin token symbol
 * @property {string|number|bigint} originAmount - Origin amount
 * @property {string} destinationSymbol - Destination token symbol
 * @property {string} destinationChain - Destination chain key
 * @property {string|number|bigint} destinationAmount - Destination amount
 * @property {string|number|bigint} slippage - Acceptable slippage
 * @property {number} [pricingType=0] - 0 = Origin, 1 = Destination (QUOTE_PRICING_ORIGIN / QUOTE_PRICING_DESTINATION)
 * @property {number} [quoteDeadlineSeconds=60] - How long to wait for a quote (0–60)
 * @property {string} originAddress - Origin address (hex)
 * @property {string} refundAddress - Refund address if tx fails (hex)
 * @property {number} [transactionDeadlineSeconds=0] - Max time for transaction to complete
 * @property {string} [message] - Optional message
 */

/**
 * @typedef {Object} QuoteFee
 * @property {string} [recipient] - Fee recipient
 * @property {string} amount - Fee amount
 * @property {string} [symbol] - Fee symbol
 */

/**
 * @typedef {Object} NewSwapResponse
 * @property {string} swap_request_id - Swap request id
 * @property {string} swap_request_expiry - When the quote expires
 * @property {QuoteFee[]} fees - Fee components
 * @property {string} signed_input_hash - Hash of input params signed by LIT
 * @property {string} transaction_hash - Transaction hash for the request
 */

// --- Fill quote ---

/**
 * @typedef {Object} FillQuoteRequestOptions
 * @property {string} swapRequestId - Swap request id from new_quote_request
 * @property {number} quoteDeadlineSeconds - Quote deadline in seconds
 * @property {string} providerRefundAddress - Provider refund address (hex)
 * @property {string} [message] - Optional message
 */

/**
 * @typedef {Object} FillQuoteResponse
 * @property {string} quote_id - Quote id
 * @property {string} transaction_hash - Transaction hash
 * @property {string} pkp_address - PKP address for the quote
 * @property {string} swap_request_id - Swap request id
 * @property {string} quote_expiry - Quote expiry time
 * @property {QuoteFee[]} fees - Fee components
 * @property {number} total_fees - Total fees
 * @property {string} signed_input_hash - Signed input hash
 */

// --- Accept quote ---

/**
 * @typedef {Object} AcceptQuoteRequestOptions
 * @property {string} quoteId - Quote id from fill_quote
 */

/**
 * @typedef {Object} AcceptQuoteResponse
 * @property {string} pkp_address - PKP address to send funds to
 */

// --- Get swap status ---

/** Swap state: pending. */
export const SWAP_STATE_PENDING = 'Pending';
/** Swap state: processing. */
export const SWAP_STATE_PROCESSING = 'Processing';
/** Swap state: success. */
export const SWAP_STATE_SUCCESS = 'Success';
/** Swap state: expired. */
export const SWAP_STATE_EXPIRED = 'Expired';
/** Swap state: refunded. */
export const SWAP_STATE_REFUNDED = 'Refunded';
/** Swap state: other (e.g. failed, cancelled). */
export const SWAP_STATE_OTHER = 'Other';

// --- Open swap requests / open quotes (contract-mirror types) ---

/**
 * @typedef {Object} SwapRequestData
 * Mirrors SwapRequestData in models.rs. Amounts are in ether (f64).
 * @property {string} from - Message sender (hex)
 * @property {string} pkp_address - PKP address for this swap request (hex)
 * @property {string} origin_symbol
 * @property {string} origin_chain
 * @property {number} origin_amount - In ether (f64)
 * @property {string} destination_symbol
 * @property {string} destination_chain
 * @property {number} destination_amount - In ether (f64)
 * @property {string|number} slippage
 * @property {number} pricing_type - 0 = Origin, 1 = Destination
 * @property {string|number} quote_deadline_seconds
 * @property {string} origin_address
 * @property {string} refund_address
 * @property {string|number} transaction_deadline_seconds
 * @property {string} message
 */

/**
 * @typedef {Object} QuoteData
 * Matches QuoteData in models.rs (from get_recent_quotes + get_requests_by_ids).
 * @property {string|number} swap_request_id
 * @property {string} provider_refund_address
 * @property {number} quote_expiry - Unix timestamp
 * @property {number} created_at - Unix timestamp
 * @property {string|number} fees_total
 * @property {SwapRequestData} swap_request_data - The swap request this quote fulfils
 */

/**
 * @typedef {Object} GetOpenSwapRequestsResponse
 * @property {SwapRequestData[]} swap_requests
 */

/**
 * @typedef {Object} GetOpenQuotesResponse
 * @property {QuoteData[]} quotes
 */

/**
 * @typedef {Object} QuoteBalancesResponse
 * Response from get_quote_balances: quote data and PKP balances on source/destination chains.
 * @property {string} pkp_address - PKP address (hex)
 * @property {string} source_chain - Origin chain key
 * @property {string} destination_chain - Destination chain key
 * @property {string} source_balance_wei - Balance in wei on source chain (native token)
 * @property {string} destination_balance_wei - Balance in wei on destination chain (native token)
 * @property {boolean} source_balance_sufficient - Whether PKP has enough balance on source chain for the swap
 * @property {boolean} destination_balance_sufficient - Whether PKP has enough balance on destination chain for the swap
 */

/**
 * @typedef {Object} SwapQuoteDetails
 * @property {string} quote_id - Quote id
 * @property {string} [origin_symbol] - Origin symbol
 * @property {string} [origin_amount] - Origin amount
 * @property {string} [destination_symbol] - Destination symbol
 * @property {string} [destination_amount] - Destination amount
 * @property {string} [pkp_address] - PKP address
 * @property {Object} [extra] - Additional fields
 */

/**
 * @typedef {Object} GetSwapStatusResponse
 * @property {string} state - One of Pending, Processing, Success, Expired, Refunded, Other
 * @property {SwapQuoteDetails|null} [details] - Swap/quote details when available
 */

/**
 * Extract error message from API error response body. Handles { error }, { message }, or tuple-style [string].
 * @param {string} text - Raw response text
 * @returns {string|null} Extracted message or null
 */
function getErrorMessageFromBody(text) {
  if (!text || !text.trim()) return null;
  try {
    const body = JSON.parse(text);
    if (!body) return null;
    if (typeof body.error === 'string' && body.error.length > 0) return body.error;
    if (typeof body.message === 'string' && body.message.length > 0) return body.message;
    if (Array.isArray(body) && body.length === 1 && typeof body[0] === 'string') return body[0];
    if (typeof body === 'string') return body;
    return null;
  } catch (_) {
    return null;
  }
}

/**
 * Parse a fetch Response: on success return JSON body; on error throw with server message in body when present.
 * API error responses include a message in the body (e.g. { error: string } or { message: string }).
 * @param {Response} res - fetch Response
 * @param {string} context - Label for the request (e.g. "token_list")
 * @returns {Promise<any>} Parsed JSON body on success
 * @throws {Error} With server error message when res.ok is false (body message included when present)
 */
async function parseResponse(res, context) {
  const text = await res.text();
  if (!res.ok) {
    const bodyMessage = getErrorMessageFromBody(text);
    const message = bodyMessage
      ? `${context}: ${bodyMessage}`
      : `${context}: ${res.status} ${res.statusText}`;
    throw new Error(message);
  }
  try {
    return text ? JSON.parse(text) : null;
  } catch (_) {
    throw new Error(`${context}: invalid JSON response`);
  }
}

export class LitSwapsApiClient {
  /**
   * @param {Object} options
   * @param {string} [options.baseUrl='http://localhost:8000'] - Base URL of the API (server root). Routes are requested at baseUrl/swaps/v1/.
   */
  constructor({ baseUrl = 'http://localhost:8000' } = {}) {
    const base = baseUrl.replace(/\/$/, '');
    this.baseUrl = `${base}/swaps/v1`;
  }

  /**
   * GET /swaps/v1/token_list
   * Returns a list of supported tokens.
   * @returns {Promise<TokenListResponse>} { tokens: TokenListItem[] }
   */
  async getTokenList() {
    const res = await fetch(`${this.baseUrl}/token_list`);
    return parseResponse(res, 'token_list');
  }

  /**
   * POST /swaps/v1/new_quote_request
   * Create a new swap request and get a swap_request_id and transaction hash.
   * @param {NewSwapRequestOptions} options
   * @returns {Promise<NewSwapResponse>}
   */
  async newQuoteRequest(options) {
    const body = {
      from: options.from,
      origin_chain: options.originChain,
      origin_symbol: options.originSymbol,
      origin_amount: Number(options.originAmount),
      destination_symbol: options.destinationSymbol,
      destination_chain: options.destinationChain,
      destination_amount: Number(options.destinationAmount),
      slippage: Number(options.slippage ?? 1),
      pricing_type: Number(options.pricingType ?? QUOTE_PRICING_ORIGIN),
      quote_deadline_seconds: Number(options.quoteDeadlineSeconds ?? 60),
      origin_address: options.originAddress,
      refund_address: options.refundAddress,
      transaction_deadline_seconds: Number(options.transactionDeadlineSeconds ?? 0),
      message: options.message ?? null,
    };
    const res = await fetch(`${this.baseUrl}/new_quote_request`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'new_quote_request');
  }

  /**
   * POST /swaps/v1/fill_quote
   * Fill a quote and get the quote_id and transaction hash.
   * @param {FillQuoteRequestOptions} options
   * @returns {Promise<FillQuoteResponse>}
   */
  async fillQuote(options) {
    const body = {
      swap_request_id: options.swapRequestId,
      quote_deadline_seconds: options.quoteDeadlineSeconds,
      provider_refund_address: options.providerRefundAddress,
      message: options.message ?? null,
    };
    const res = await fetch(`${this.baseUrl}/fill_quote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'fill_quote');
  }

  /**
   * POST /swaps/v1/accept_quote
   * Accept a quote and get the PKP address to send funds to.
   * @param {AcceptQuoteRequestOptions} options
   * @returns {Promise<AcceptQuoteResponse>}
   */
  async acceptQuote(options) {
    const body = { quote_id: options.quoteId };
    const res = await fetch(`${this.baseUrl}/accept_quote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    return parseResponse(res, 'accept_quote');
  }

  /**
   * GET /swaps/v1/get_contract_address
   * Returns the quote storage contract address (hex string).
   * @returns {Promise<string>} Contract address
   */
  async getContractAddress() {
    const res = await fetch(`${this.baseUrl}/get_contract_address`);
    return parseResponse(res, 'get_contract_address');
  }

  /**
   * GET /swaps/v1/get_swap_status/<quote_id>
   * Get the status of a swap by quote id.
   * @param {string} quoteId - Quote id from fill_quote
   * @returns {Promise<GetSwapStatusResponse>} { state, details? }
   */
  async getSwapStatus(quoteId) {
    const res = await fetch(
      `${this.baseUrl}/get_swap_status/${encodeURIComponent(quoteId)}`
    );
    return parseResponse(res, 'get_swap_status');
  }

  /**
   * GET /swaps/v1/get_open_swap_requests
   * Returns open swap requests from the contract via getRecentSwapRequests(count).
   * @returns {Promise<GetOpenSwapRequestsResponse>} { swap_requests: SwapRequestData[] }
   */
  async getOpenSwapRequests() {
    const res = await fetch(`${this.baseUrl}/get_open_swap_requests`);
    return parseResponse(res, 'get_open_swap_requests');
  }

  /**
   * GET /swaps/v1/get_open_quotes
   * Returns open quotes from the contract via getRecentQuotes(count).
   * @returns {Promise<GetOpenQuotesResponse>} { quotes: QuoteData[] }
   */
  async getOpenQuotes() {
    const res = await fetch(`${this.baseUrl}/get_open_quotes`);
    return parseResponse(res, 'get_open_quotes');
  }

  /**
   * GET /swaps/v1/get_quote_balances/<quote_id>
   * Get quote data and PKP balance on source and destination chains.
   * @param {string} quoteId - Quote id (decimal string, e.g. from fill_quote)
   * @returns {Promise<QuoteBalancesResponse>} { pkp_address, source_chain, destination_chain, source_balance_wei, destination_balance_wei, source_balance_sufficient, destination_balance_sufficient }
   */
  async getQuoteBalances(quoteId) {
    const res = await fetch(
      `${this.baseUrl}/get_quote_balances/${encodeURIComponent(quoteId)}`
    );
    return parseResponse(res, 'get_quote_balances');
  }

  /**
   * GET /swaps/v1/attempt_swap_request/<quote_id>
   * Attempt to execute the swap for the given quote (sends origin/destination transfers).
   * @param {string} quoteId - Quote id from fill_quote
   * @returns {Promise<string>} Response body (empty string on success)
   * @throws {Error} On non-OK response (e.g. 402 Payment Required if insufficient balance)
   */
  async attemptSwapRequest(quoteId) {
    const res = await fetch(
      `${this.baseUrl}/attempt_swap_request/${encodeURIComponent(quoteId)}`
    );
    return parseResponse(res, 'attempt_swap_request');
  }
}

/**
 * Factory for a swaps API client.
 * @param {string} [baseUrl='http://localhost:8000'] - Server root URL
 * @returns {LitSwapsApiClient}
 */
export function createSwapsClient(baseUrl = 'http://localhost:8000') {
  return new LitSwapsApiClient({ baseUrl });
}

export default LitSwapsApiClient;
