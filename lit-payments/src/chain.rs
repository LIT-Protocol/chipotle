//! LITKEY on-chain payment helpers.
//!
//! Browser payments submit the exact transaction hash to the backend. The backend
//! fetches that receipt, verifies the configured gateway emitted the expected
//! `Payment` event, and credits idempotently from that event.

use alloy_dyn_abi::DynSolType;
use alloy_primitives::{Address, B256, Bytes, U256, keccak256};
use anyhow::{Context, Result};
use lit_billing_core::{StripeClient, balance, customer};
use reqwest::Client;
use rocket::State;
use rocket::http::Status;
use rocket::serde::json::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::time::Duration as StdDuration;
pub const BASE_CHAIN_ID: i64 = 8453;
pub const LITKEY_TOKEN_ADDRESS: &str = "0xf732a566121fa6362e9e0fbdd6d66e5c8c925e49";
pub const DEFAULT_GATEWAY_ADDRESS: &str = "0xa2d54cd1d1df1735718a857ac49caf9ecab0093b";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainConfig {
    pub chain_id: i64,
    pub alchemy_https_url: String,
    pub gateway_address: Address,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentLog {
    pub chain_id: i64,
    pub gateway_address: Address,
    pub wallet: Address,
    pub payer: Address,
    pub amount_wei: U256,
    pub tx_hash: B256,
    pub log_index: u64,
    pub block_number: u64,
}

impl PaymentLog {
    pub fn idempotency_key(&self) -> String {
        payment_idempotency_key(self.chain_id, self.tx_hash, self.log_index)
    }

    pub fn gateway_address(&self) -> String {
        format_address(self.gateway_address)
    }

    pub fn wallet_address(&self) -> String {
        format_address(self.wallet)
    }

    pub fn payer_address(&self) -> String {
        format_address(self.payer)
    }

    pub fn amount_wei_string(&self) -> String {
        self.amount_wei.to_string()
    }
}

pub fn payment_idempotency_key(chain_id: i64, tx_hash: B256, log_index: u64) -> String {
    format!("litkey:{chain_id}:{tx_hash:#x}:{log_index}")
}

pub fn format_address(address: Address) -> String {
    format!("{address:#x}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaymentStatus {
    Credited,
    Dust,
    Paused,
    NoCustomer,
}

impl PaymentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Credited => "credited",
            Self::Dust => "dust",
            Self::Paused => "paused",
            Self::NoCustomer => "no_customer",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewLitkeyPayment {
    pub log: PaymentLog,
    pub usd_wei_per_litkey: Option<String>,
    pub discount_basis_points: i64,
    pub cents_credited: i64,
    pub status: PaymentStatus,
    pub stripe_customer_id: Option<String>,
    pub stripe_balance_transaction_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripeCreditAction {
    pub customer_id: String,
    pub amount_cents: i64,
    pub description: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentDecision {
    pub status: PaymentStatus,
    pub payment: NewLitkeyPayment,
    pub stripe_credit: Option<StripeCreditAction>,
}

pub fn litkey_payment_description(
    log: &PaymentLog,
    usd_wei_per_litkey: &str,
    discount_basis_points: i64,
) -> String {
    format!(
        "LITKEY payment tx={:#x} wallet={} payer={} amount={} wei rate={} usd_wei_per_litkey discount_bps={}",
        log.tx_hash,
        log.wallet_address(),
        log.payer_address(),
        log.amount_wei_string(),
        usd_wei_per_litkey,
        discount_basis_points
    )
}

pub fn classify_litkey_payment(
    log: &PaymentLog,
    rate: Option<&crate::rate::LitkeyRate>,
    discount_basis_points: i64,
    stripe_customer_id: Option<String>,
) -> Result<PaymentDecision> {
    let Some(rate) = rate.filter(|rate| !rate.stale) else {
        let payment = NewLitkeyPayment {
            log: log.clone(),
            usd_wei_per_litkey: None,
            discount_basis_points,
            cents_credited: 0,
            status: PaymentStatus::Paused,
            stripe_customer_id: None,
            stripe_balance_transaction_id: None,
        };
        return Ok(PaymentDecision {
            status: PaymentStatus::Paused,
            payment,
            stripe_credit: None,
        });
    };

    let cents_credited = crate::rate::litkey_wei_to_credit_cents(
        &log.amount_wei_string(),
        &rate.usd_wei_per_litkey,
        discount_basis_points,
    )?;
    if cents_credited == 0 {
        let payment = NewLitkeyPayment {
            log: log.clone(),
            usd_wei_per_litkey: Some(rate.usd_wei_per_litkey.clone()),
            discount_basis_points,
            cents_credited,
            status: PaymentStatus::Dust,
            stripe_customer_id: None,
            stripe_balance_transaction_id: None,
        };
        return Ok(PaymentDecision {
            status: PaymentStatus::Dust,
            payment,
            stripe_credit: None,
        });
    }

    let Some(customer_id) = stripe_customer_id else {
        let payment = NewLitkeyPayment {
            log: log.clone(),
            usd_wei_per_litkey: Some(rate.usd_wei_per_litkey.clone()),
            discount_basis_points,
            cents_credited,
            status: PaymentStatus::NoCustomer,
            stripe_customer_id: None,
            stripe_balance_transaction_id: None,
        };
        return Ok(PaymentDecision {
            status: PaymentStatus::NoCustomer,
            payment,
            stripe_credit: None,
        });
    };

    let stripe_credit = StripeCreditAction {
        customer_id: customer_id.clone(),
        amount_cents: -cents_credited,
        description: litkey_payment_description(
            log,
            &rate.usd_wei_per_litkey,
            discount_basis_points,
        ),
        idempotency_key: log.idempotency_key(),
    };
    let payment = NewLitkeyPayment {
        log: log.clone(),
        usd_wei_per_litkey: Some(rate.usd_wei_per_litkey.clone()),
        discount_basis_points,
        cents_credited,
        status: PaymentStatus::Credited,
        stripe_customer_id: Some(customer_id),
        stripe_balance_transaction_id: None,
    };
    Ok(PaymentDecision {
        status: PaymentStatus::Credited,
        payment,
        stripe_credit: Some(stripe_credit),
    })
}

pub async fn payment_exists(pool: &PgPool, log: &PaymentLog) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM litkey_payments WHERE chain_id = $1 AND tx_hash = $2 AND log_index = $3)",
    )
    .bind(log.chain_id)
    .bind(format!("{:#x}", log.tx_hash))
    .bind(log.log_index as i64)
    .fetch_one(pool)
    .await
    .context("checking litkey payment idempotency")?;
    Ok(exists)
}

pub async fn insert_payment(pool: &PgPool, payment: &NewLitkeyPayment) -> Result<bool> {
    let inserted = sqlx::query_scalar::<_, i64>(
        "INSERT INTO litkey_payments (
            chain_id, gateway_address, tx_hash, log_index, block_number,
            wallet_address, payer_address, litkey_amount_wei, usd_wei_per_litkey,
            discount_basis_points, cents_credited, status, stripe_customer_id,
            stripe_balance_transaction_id
         ) VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8::numeric, $9::numeric,
            $10, $11, $12, $13, $14
         ) ON CONFLICT (chain_id, tx_hash, log_index) DO NOTHING
         RETURNING id",
    )
    .bind(payment.log.chain_id)
    .bind(payment.log.gateway_address())
    .bind(format!("{:#x}", payment.log.tx_hash))
    .bind(payment.log.log_index as i64)
    .bind(payment.log.block_number as i64)
    .bind(payment.log.wallet_address())
    .bind(payment.log.payer_address())
    .bind(payment.log.amount_wei_string())
    .bind(payment.usd_wei_per_litkey.as_deref())
    .bind(payment.discount_basis_points)
    .bind(payment.cents_credited)
    .bind(payment.status.as_str())
    .bind(payment.stripe_customer_id.as_deref())
    .bind(payment.stripe_balance_transaction_id.as_deref())
    .fetch_optional(pool)
    .await
    .context("inserting litkey payment")?;
    Ok(inserted.is_some())
}

/// Fill in the Stripe `balance_transaction` id on a previously-inserted
/// credited row, promoting it from "partial" to complete. Keyed by the
/// on-chain `(chain_id, tx_hash, log_index)` identity so both the live claim
/// path and the reconciler converge on the same row (CPL-375).
///
/// Guarded on `stripe_balance_transaction_id IS NULL` so completion is
/// write-once: once a row records a balance transaction, a later caller (the
/// live path and reconciler can race on the same row) cannot clobber that id
/// with a different one — a duplicate credit would then stay visible rather
/// than being papered over. Affecting zero rows is therefore the normal
/// idempotent outcome, not an error, so we don't assert a row count.
pub async fn mark_payment_credited(
    pool: &PgPool,
    log: &PaymentLog,
    stripe_balance_transaction_id: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE litkey_payments
            SET stripe_balance_transaction_id = $1
          WHERE chain_id = $2 AND tx_hash = $3 AND log_index = $4
            AND stripe_balance_transaction_id IS NULL",
    )
    .bind(stripe_balance_transaction_id)
    .bind(log.chain_id)
    .bind(format!("{:#x}", log.tx_hash))
    .bind(log.log_index as i64)
    .execute(pool)
    .await
    .context("marking litkey payment credited")?;
    Ok(())
}

/// Stripe drops a given Idempotency-Key after ~24h. Past this age the stable
/// `litkey:{chain}:{tx}:{log_index}` key can no longer be trusted to dedupe,
/// so replaying a partial credit risks double-crediting. Both the background
/// reconciler and the synchronous claim path refuse to auto-complete a
/// partial older than this; such rows are surfaced for manual repair.
pub const PARTIAL_CREDIT_IDEMPOTENCY_WINDOW_HOURS: i64 = 23;

/// A `credited` `litkey_payments` row whose Stripe `balance_transaction`
/// write never landed (crash or Stripe error between INSERT and the credit).
/// Carries everything needed to replay the credit with the original
/// idempotency key.
#[derive(Clone, Debug)]
pub struct PartialLitkeyCredit {
    pub log: PaymentLog,
    pub usd_wei_per_litkey: String,
    pub discount_basis_points: i64,
    pub cents_credited: i64,
    pub stripe_customer_id: String,
    pub credited_at: time::OffsetDateTime,
}

impl PartialLitkeyCredit {
    /// The stable Stripe Idempotency-Key for this credit. Reusing it is what
    /// makes a replay safe: if the original credit landed, Stripe dedupes; if
    /// it did not, Stripe credits fresh — never both.
    pub fn idempotency_key(&self) -> String {
        self.log.idempotency_key()
    }

    /// Stripe balance delta (negative cents credit the customer), matching
    /// [`classify_litkey_payment`].
    pub fn amount_cents(&self) -> i64 {
        -self.cents_credited
    }

    pub fn description(&self) -> String {
        litkey_payment_description(
            &self.log,
            &self.usd_wei_per_litkey,
            self.discount_basis_points,
        )
    }

    /// True once the partial is too old for its idempotency key to be trusted
    /// to dedupe — replaying past this point risks a double credit, so callers
    /// must refuse and leave the row for manual repair. See
    /// [`PARTIAL_CREDIT_IDEMPOTENCY_WINDOW_HOURS`].
    pub fn past_idempotency_window(&self, now: time::OffsetDateTime) -> bool {
        (now - self.credited_at).whole_hours() >= PARTIAL_CREDIT_IDEMPOTENCY_WINDOW_HOURS
    }
}

/// Column tuple shared by the partial-credit queries. The rate/customer
/// columns read as `Option` because they are nullable in the schema, though a
/// `credited` row always has both (enforced by the status-fields CHECK); a
/// violated invariant then degrades to a default instead of panicking.
type PartialCreditRow = (
    i64,
    String,
    String,
    i64,
    i64,
    String,
    String,
    String,
    Option<String>,
    i64,
    i64,
    Option<String>,
    time::OffsetDateTime,
);

const PARTIAL_CREDIT_COLUMNS: &str = "chain_id, gateway_address, tx_hash, log_index, block_number, \
     wallet_address, payer_address, litkey_amount_wei::text, \
     usd_wei_per_litkey::text, discount_basis_points, cents_credited, \
     stripe_customer_id, credited_at";

fn partial_credit_from_row(r: PartialCreditRow) -> Result<PartialLitkeyCredit> {
    let (
        chain_id,
        gateway_address,
        tx_hash,
        log_index,
        block_number,
        wallet_address,
        payer_address,
        litkey_amount_wei,
        usd_wei_per_litkey,
        discount_basis_points,
        cents_credited,
        stripe_customer_id,
        credited_at,
    ) = r;
    let log = PaymentLog {
        chain_id,
        gateway_address: gateway_address
            .parse()
            .context("partial credit gateway_address")?,
        wallet: wallet_address.parse().context("partial credit wallet")?,
        payer: payer_address.parse().context("partial credit payer")?,
        amount_wei: litkey_amount_wei
            .parse()
            .context("partial credit litkey_amount_wei")?,
        tx_hash: tx_hash.parse().context("partial credit tx_hash")?,
        log_index: log_index as u64,
        block_number: block_number as u64,
    };
    Ok(PartialLitkeyCredit {
        log,
        // A `credited` row always has a rate and a customer (enforced by
        // `litkey_payments_status_fields_check`); default defensively rather
        // than panic if that invariant is ever violated.
        usd_wei_per_litkey: usd_wei_per_litkey.unwrap_or_default(),
        discount_basis_points,
        cents_credited,
        stripe_customer_id: stripe_customer_id.unwrap_or_default(),
        credited_at,
    })
}

/// List every credited row still awaiting its `balance_transactions` write
/// (`stripe_balance_transaction_id IS NULL`), oldest first. The reconciler
/// age-gates each one before replaying the Stripe credit.
pub async fn list_partial_litkey_credits(pool: &PgPool) -> Result<Vec<PartialLitkeyCredit>> {
    let query = format!(
        "SELECT {PARTIAL_CREDIT_COLUMNS}
           FROM litkey_payments
          WHERE status = 'credited' AND stripe_balance_transaction_id IS NULL
          ORDER BY credited_at ASC"
    );
    let rows = sqlx::query_as::<_, PartialCreditRow>(&query)
        .fetch_all(pool)
        .await
        .context("listing partial litkey credits")?;
    rows.into_iter().map(partial_credit_from_row).collect()
}

/// Fetch the most recent partial credit for a `(tx_hash, wallet)`, if any.
/// Used by the claim path to finish a credit synchronously on re-claim
/// instead of waiting for the background reconciler. Matches the chain and
/// wallet scoping of [`lookup_payment_status`].
pub async fn find_partial_litkey_credit(
    pool: &PgPool,
    tx_hash: &str,
    wallet: &str,
) -> Result<Option<PartialLitkeyCredit>> {
    let query = format!(
        "SELECT {PARTIAL_CREDIT_COLUMNS}
           FROM litkey_payments
          WHERE chain_id = $1 AND tx_hash = $2 AND wallet_address = $3
            AND status = 'credited' AND stripe_balance_transaction_id IS NULL
          ORDER BY credited_at DESC
          LIMIT 1"
    );
    let row = sqlx::query_as::<_, PartialCreditRow>(&query)
        .bind(BASE_CHAIN_ID)
        .bind(tx_hash)
        .bind(wallet)
        .fetch_optional(pool)
        .await
        .context("finding partial litkey credit")?;
    row.map(partial_credit_from_row).transpose()
}

/// Replay the Stripe credit for a partial row and record the balance
/// transaction id (write-once via [`mark_payment_credited`]). Shared by the
/// reconciler and the synchronous claim path. Reuses the original idempotency
/// key, so it dedupes if the credit already landed. Callers must enforce
/// their own age policy first (see
/// [`PartialLitkeyCredit::past_idempotency_window`]); this issues the Stripe
/// call unconditionally.
pub async fn complete_partial_litkey_credit(
    stripe: &StripeClient,
    pool: &PgPool,
    partial: &PartialLitkeyCredit,
) -> Result<()> {
    let balance_transaction_id = balance::write_transaction(
        stripe,
        &partial.stripe_customer_id,
        partial.amount_cents(),
        &partial.description(),
        Some(&partial.idempotency_key()),
    )
    .await?;
    mark_payment_credited(pool, &partial.log, &balance_transaction_id).await?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct LitkeyPaymentConfigResponse {
    pub chain_id: i64,
    pub token_address: String,
    pub gateway_address: String,
}

#[derive(Debug, Serialize)]
pub struct LitkeyPaymentStatusResponse {
    pub found: bool,
    pub status: Option<String>,
    pub cents_credited: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub credited_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Deserialize)]
pub struct LitkeyPaymentClaimRequest {
    pub tx_hash: String,
    pub wallet: String,
}

type ApiError = (Status, Json<crate::portal::types::ErrorResponse>);
type ApiResult<T> = std::result::Result<Json<T>, ApiError>;

fn api_err(status: Status, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(crate::portal::types::ErrorResponse {
            error: message.into(),
        }),
    )
}

fn api_server_err(e: impl std::fmt::Display + std::fmt::Debug) -> ApiError {
    tracing::warn!(error = %e, error_debug = ?e, "litkey payment API route internal error");
    api_err(Status::InternalServerError, "internal error")
}

pub fn canonical_tx_hash_param(tx_hash: &str) -> std::result::Result<String, String> {
    tx_hash
        .trim()
        .parse::<B256>()
        .map(|hash| format!("{hash:#x}"))
        .map_err(|_| "tx_hash must be a 0x transaction hash".to_string())
}

pub fn canonical_wallet_param(wallet: &str) -> std::result::Result<String, String> {
    wallet
        .trim()
        .parse::<Address>()
        .map(|address| format!("{address:#x}"))
        .map_err(|_| "wallet must be a 0x Ethereum address".to_string())
}

/// `GET /api/litkey/payment-config` — public on-chain config for browser payments.
#[rocket::get("/api/litkey/payment-config")]
pub fn get_payment_config(
    config: &State<crate::config::Config>,
) -> ApiResult<LitkeyPaymentConfigResponse> {
    let Some(chain) = config.litkey_chain.as_ref() else {
        return Err(api_err(
            Status::ServiceUnavailable,
            "LITKEY chain verification is not configured",
        ));
    };
    Ok(Json(LitkeyPaymentConfigResponse {
        chain_id: chain.chain_id,
        token_address: LITKEY_TOKEN_ADDRESS.to_string(),
        gateway_address: format_address(chain.gateway_address),
    }))
}

pub async fn lookup_payment_status(
    pool: &PgPool,
    tx_hash: &str,
    wallet: &str,
) -> Result<Option<(String, i64, time::OffsetDateTime)>> {
    sqlx::query_as::<_, (String, i64, time::OffsetDateTime)>(
        "SELECT status, cents_credited, credited_at
         FROM litkey_payments
         WHERE chain_id = $1 AND tx_hash = $2 AND wallet_address = $3
         ORDER BY credited_at DESC
         LIMIT 1",
    )
    .bind(BASE_CHAIN_ID)
    .bind(tx_hash)
    .bind(wallet)
    .fetch_optional(pool)
    .await
    .context("looking up litkey payment status")
}

/// `POST /api/litkey/payment-claim` — verify a submitted tx hash and credit it.
///
/// The browser already has the exact payment tx hash after `gateway.pay(...)`.
/// This endpoint fetches that receipt directly, verifies the configured gateway
/// emitted `Payment(wallet, payer, amount)`, and then runs the shared idempotent
/// crediting handler. Base reorg risk is intentionally not modeled in this
/// browser-driven claim path.
#[rocket::post("/api/litkey/payment-claim", format = "json", data = "<req>")]
pub async fn claim_payment(
    req: Json<LitkeyPaymentClaimRequest>,
    pool: &State<PgPool>,
    stripe: &State<StripeClient>,
    config: &State<crate::config::Config>,
) -> ApiResult<LitkeyPaymentStatusResponse> {
    let Some(chain_config) = config.litkey_chain.as_ref() else {
        return Err(api_err(
            Status::ServiceUnavailable,
            "LITKEY chain verification is not configured",
        ));
    };
    let tx_hash =
        canonical_tx_hash_param(&req.tx_hash).map_err(|e| api_err(Status::BadRequest, e))?;
    let wallet = canonical_wallet_param(&req.wallet).map_err(|e| api_err(Status::BadRequest, e))?;

    let rpc = HttpGatewayRpc::new(chain_config);
    let claimed = claim_litkey_payment_tx(
        pool,
        stripe,
        &rpc,
        chain_config,
        &tx_hash,
        &wallet,
        config.litkey_discount_basis_points,
    )
    .await
    .map_err(api_server_err)?;
    Ok(Json(claimed))
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct RpcLog {
    address: Address,
    #[serde(rename = "blockNumber")]
    #[serde(with = "alloy_serde::quantity::opt")]
    block_number: Option<u64>,
    #[serde(rename = "logIndex")]
    #[serde(with = "alloy_serde::quantity::opt")]
    log_index: Option<u64>,
    #[serde(rename = "transactionHash")]
    transaction_hash: Option<B256>,
    topics: Vec<B256>,
    data: Bytes,
    removed: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RpcReceipt {
    #[serde(with = "alloy_serde::quantity::opt")]
    status: Option<u64>,
    logs: Vec<RpcLog>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    pub address: Address,
    pub block_number: Option<u64>,
    pub log_index: Option<u64>,
    pub transaction_hash: Option<B256>,
    pub topics: Vec<B256>,
    pub data: Bytes,
    pub removed: Option<bool>,
}

impl From<RpcLog> for Log {
    fn from(log: RpcLog) -> Self {
        Self {
            address: log.address,
            block_number: log.block_number,
            log_index: log.log_index,
            transaction_hash: log.transaction_hash,
            topics: log.topics,
            data: log.data,
            removed: log.removed,
        }
    }
}

pub struct HttpGatewayRpc {
    client: Client,
    https_url: String,
}

impl HttpGatewayRpc {
    pub fn new(config: &ChainConfig) -> Self {
        Self {
            client: rpc_http_client(),
            https_url: config.alchemy_https_url.clone(),
        }
    }
}

fn rpc_http_client() -> Client {
    Client::builder()
        .connect_timeout(StdDuration::from_secs(5))
        .timeout(StdDuration::from_secs(30))
        .build()
        .expect("valid LITKEY RPC HTTP client")
}

impl HttpGatewayRpc {
    async fn rpc<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T> {
        let response = self
            .client
            .post(&self.https_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params,
            }))
            .send()
            .await
            .with_context(|| format!("sending {method} to LITKEY HTTPS RPC"))?
            .error_for_status()
            .with_context(|| format!("LITKEY HTTPS RPC {method} returned non-success status"))?
            .json::<JsonRpcResponse<T>>()
            .await
            .with_context(|| format!("decoding LITKEY HTTPS RPC {method} response"))?;

        if let Some(error) = response.error {
            anyhow::bail!(
                "LITKEY HTTPS RPC {method} failed: {} ({})",
                error.message,
                error.code
            );
        }
        response
            .result
            .with_context(|| format!("LITKEY HTTPS RPC {method} response missing result"))
    }
}

impl HttpGatewayRpc {
    async fn transaction_receipt(&self, tx_hash: &str) -> Result<Option<RpcReceipt>> {
        self.rpc("eth_getTransactionReceipt", json!([tx_hash]))
            .await
    }
}

pub const PAYMENT_EVENT_SIGNATURE: &str = "Payment(address,address,uint256)";

pub fn payment_event_topic() -> B256 {
    keccak256(PAYMENT_EVENT_SIGNATURE.as_bytes())
}

pub fn parse_gateway_payment_log(
    chain_id: i64,
    gateway_address: Address,
    log: Log,
) -> Result<Option<PaymentLog>> {
    if log.topics.first().copied() != Some(payment_event_topic()) {
        return Ok(None);
    }
    if log.address != gateway_address {
        anyhow::bail!(
            "Payment log address {} did not match configured gateway {}",
            format_address(log.address),
            format_address(gateway_address)
        );
    }
    if log.topics.len() != 3 {
        anyhow::bail!(
            "indexed Payment log must have exactly 3 topics; got {}",
            log.topics.len()
        );
    }
    let tx_hash = log
        .transaction_hash
        .context("payment log missing tx hash")?;
    let log_index = log.log_index.context("payment log missing log index")?;
    let block_number = log
        .block_number
        .context("payment log missing block number")?;

    let wallet = Address::from_slice(&log.topics[1].as_slice()[12..]);
    let payer = Address::from_slice(&log.topics[2].as_slice()[12..]);
    let decoded = DynSolType::Uint(256)
        .abi_decode(&log.data)
        .context("decoding indexed Payment amount")?;
    let (amount_wei, _) = decoded.as_uint().context("Payment amount not uint")?;

    Ok(Some(PaymentLog {
        chain_id,
        gateway_address,
        wallet,
        payer,
        amount_wei,
        tx_hash,
        log_index,
        block_number,
    }))
}

pub fn select_matching_payment_log(
    chain_id: i64,
    gateway_address: Address,
    logs: Vec<Log>,
    wallet: Address,
) -> Result<Option<PaymentLog>> {
    let mut matches = Vec::new();
    for log in logs {
        let Some(payment) = parse_gateway_payment_log(chain_id, gateway_address, log)? else {
            continue;
        };
        if payment.wallet == wallet {
            matches.push(payment);
        }
    }
    matches.sort_by_key(|log| log.log_index);
    Ok(matches.into_iter().next())
}

fn payment_status_response_from_row(
    row: Option<(String, i64, time::OffsetDateTime)>,
) -> LitkeyPaymentStatusResponse {
    match row {
        Some((status, cents_credited, credited_at)) => LitkeyPaymentStatusResponse {
            found: true,
            status: Some(status),
            cents_credited: Some(cents_credited),
            credited_at: Some(credited_at),
        },
        None => LitkeyPaymentStatusResponse {
            found: false,
            status: None,
            cents_credited: None,
            credited_at: None,
        },
    }
}

async fn claim_litkey_payment_tx(
    pool: &PgPool,
    stripe: &StripeClient,
    rpc: &HttpGatewayRpc,
    config: &ChainConfig,
    tx_hash: &str,
    wallet: &str,
    discount_basis_points: i64,
) -> Result<LitkeyPaymentStatusResponse> {
    // A partial credit (row inserted, Stripe write interrupted — CPL-375) is a
    // `credited` row that `lookup_payment_status` would report as done even
    // though the balance_transactions write never landed. On re-claim, finish
    // it synchronously (reusing the original idempotency key, so it dedupes if
    // the credit actually landed) rather than waiting for the background
    // reconciler tick, so the response reflects a truly-landed credit. Past
    // the idempotency window a replay could double-credit, so we leave those
    // for the reconciler to surface for manual repair and just report status.
    if let Some(partial) = find_partial_litkey_credit(pool, tx_hash, wallet).await? {
        if !partial.past_idempotency_window(time::OffsetDateTime::now_utc()) {
            complete_partial_litkey_credit(stripe, pool, &partial).await?;
        }
        return Ok(payment_status_response_from_row(
            lookup_payment_status(pool, tx_hash, wallet).await?,
        ));
    }
    if let Some(row) = lookup_payment_status(pool, tx_hash, wallet).await? {
        return Ok(payment_status_response_from_row(Some(row)));
    }

    let Some(receipt) = rpc.transaction_receipt(tx_hash).await? else {
        return Ok(LitkeyPaymentStatusResponse {
            found: false,
            status: Some("pending_receipt".to_string()),
            cents_credited: None,
            credited_at: None,
        });
    };
    if receipt.status != Some(1) {
        return Ok(LitkeyPaymentStatusResponse {
            found: false,
            status: Some("tx_failed".to_string()),
            cents_credited: None,
            credited_at: None,
        });
    }

    let wallet_address = wallet
        .parse::<Address>()
        .context("canonical wallet failed to parse as address")?;
    let logs = receipt.logs.into_iter().map(Into::into).collect();
    let Some(payment) = select_matching_payment_log(
        config.chain_id,
        config.gateway_address,
        logs,
        wallet_address,
    )?
    else {
        return Ok(LitkeyPaymentStatusResponse {
            found: false,
            status: None,
            cents_credited: None,
            credited_at: None,
        });
    };

    tracing::info!(
        tx_hash,
        log_index = payment.log_index,
        block_number = payment.block_number,
        wallet,
        "claiming LITKEY payment from submitted tx hash"
    );
    handle_confirmed_litkey_payment(pool, stripe, &payment, discount_basis_points).await?;
    Ok(payment_status_response_from_row(
        lookup_payment_status(pool, tx_hash, wallet).await?,
    ))
}

pub async fn handle_confirmed_litkey_payment(
    pool: &PgPool,
    stripe: &StripeClient,
    log: &PaymentLog,
    discount_basis_points: i64,
) -> Result<()> {
    if payment_exists(pool, log).await? {
        return Ok(());
    }

    let rate = crate::rate::get_current(pool).await?;
    let preliminary = classify_litkey_payment(log, rate.as_ref(), discount_basis_points, None)?;
    let customer_id = match preliminary.status {
        PaymentStatus::Paused | PaymentStatus::Dust => None,
        PaymentStatus::NoCustomer | PaymentStatus::Credited => {
            customer::find_by_wallet(stripe, &log.wallet_address()).await?
        }
    };
    let decision = classify_litkey_payment(log, rate.as_ref(), discount_basis_points, customer_id)?;

    // CPL-375: insert the row BEFORE the Stripe credit so the DB record is
    // the idempotency guard, not an afterthought. For a credited payment the
    // row lands "partial" (`stripe_balance_transaction_id` NULL) until the
    // credit is confirmed; a crash after the Stripe call can no longer strand
    // a credit with no DB record, so a re-claim past Stripe's ~24h
    // idempotency-key TTL short-circuits on `payment_exists` instead of
    // double-crediting. The `litkey_reconciler` completes any partial row
    // (retrying with the same idempotency key, which dedupes at Stripe).
    let inserted = insert_payment(pool, &decision.payment).await?;
    if !inserted {
        // A concurrent claim of the same event already recorded it (ON
        // CONFLICT DO NOTHING). That claim owns the credit; nothing to do.
        return Ok(());
    }

    if let Some(credit) = &decision.stripe_credit {
        let balance_transaction_id = balance::write_transaction(
            stripe,
            &credit.customer_id,
            credit.amount_cents,
            &credit.description,
            Some(&credit.idempotency_key),
        )
        .await?;
        mark_payment_credited(pool, log, &balance_transaction_id).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_dyn_abi::DynSolValue;
    use std::str::FromStr;

    #[test]
    fn formats_deterministic_stripe_idempotency_key() {
        let tx_hash =
            B256::from_str("0x1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap();
        assert_eq!(
            payment_idempotency_key(8453, tx_hash, 7),
            "litkey:8453:0x1111111111111111111111111111111111111111111111111111111111111111:7"
        );
    }

    // CPL-375: the reconciler must replay the EXACT Stripe call the live claim
    // path issued — same idempotency key (so Stripe dedupes), same amount, same
    // description. A partial row reconstructed by the reconciler is derived from
    // the same fields `classify_litkey_payment` persisted, so the derived credit
    // parameters must round-trip identically.
    #[test]
    fn partial_credit_replays_the_same_stripe_call_as_the_live_path() {
        let log = sample_payment_log();
        let discount_bps = 2_000;
        let decision = classify_litkey_payment(
            &log,
            Some(&fresh_rate()),
            discount_bps,
            Some("cus_123".to_string()),
        )
        .unwrap();
        let credit = decision
            .stripe_credit
            .expect("a customer with a live rate should credit");

        // Rebuild the partial the reconciler would read back from the row.
        let partial = PartialLitkeyCredit {
            log: log.clone(),
            usd_wei_per_litkey: decision.payment.usd_wei_per_litkey.clone().unwrap(),
            discount_basis_points: decision.payment.discount_basis_points,
            cents_credited: decision.payment.cents_credited,
            stripe_customer_id: decision.payment.stripe_customer_id.clone().unwrap(),
            credited_at: time::OffsetDateTime::UNIX_EPOCH,
        };

        assert_eq!(partial.idempotency_key(), credit.idempotency_key);
        assert_eq!(partial.idempotency_key(), log.idempotency_key());
        assert_eq!(partial.amount_cents(), credit.amount_cents);
        assert_eq!(partial.description(), credit.description);
        assert_eq!(partial.stripe_customer_id, credit.customer_id);
    }

    // CPL-375: both the reconciler and the synchronous claim path must refuse
    // to replay a credit once the Stripe idempotency key can no longer dedupe,
    // and must still replay just inside the window.
    #[test]
    fn partial_credit_idempotency_window_gate() {
        let make = |credited_at| PartialLitkeyCredit {
            log: sample_payment_log(),
            usd_wei_per_litkey: "1000000000000000000".to_string(),
            discount_basis_points: 2_000,
            cents_credited: 125,
            stripe_customer_id: "cus_123".to_string(),
            credited_at,
        };
        let now = time::OffsetDateTime::UNIX_EPOCH + time::Duration::days(30);

        assert!(
            !make(now - time::Duration::hours(22)).past_idempotency_window(now),
            "22h old is still inside the window"
        );
        assert!(
            make(now - time::Duration::hours(23)).past_idempotency_window(now),
            "23h old is at the window edge and must be refused"
        );
        assert!(
            make(now - time::Duration::hours(48)).past_idempotency_window(now),
            "well past the window must be refused"
        );
    }

    #[test]
    fn lowercases_and_prefixes_ethereum_addresses() {
        let address = Address::from_str("0xA2D54CD1D1dF1735718A857aC49CaF9ECaB0093b").unwrap();
        assert_eq!(
            format_address(address),
            "0xa2d54cd1d1df1735718a857ac49caf9ecab0093b"
        );
    }

    #[test]
    fn public_status_params_are_canonicalized_before_lookup() {
        assert_eq!(
            canonical_wallet_param(" 0xA2D54CD1D1dF1735718A857aC49CaF9ECaB0093b ").unwrap(),
            "0xa2d54cd1d1df1735718a857ac49caf9ecab0093b"
        );
        assert_eq!(
            canonical_tx_hash_param(
                " 0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA "
            )
            .unwrap(),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert!(canonical_wallet_param("not-a-wallet").is_err());
        assert!(canonical_tx_hash_param("0x1234").is_err());
    }

    #[test]
    fn payment_log_normalizes_addresses_and_amounts_for_processing() {
        let wallet = Address::from_str("0xA2D54CD1D1dF1735718A857aC49CaF9ECaB0093b").unwrap();
        let payer = Address::from_str("0x000000000000000000000000000000000000dEaD").unwrap();
        let log = PaymentLog {
            chain_id: 8453,
            gateway_address: wallet,
            wallet,
            payer,
            amount_wei: U256::from_str("1000000000000000000").unwrap(),
            tx_hash: B256::ZERO,
            log_index: 3,
            block_number: 123,
        };

        assert_eq!(
            log.wallet_address(),
            "0xa2d54cd1d1df1735718a857ac49caf9ecab0093b"
        );
        assert_eq!(
            log.payer_address(),
            "0x000000000000000000000000000000000000dead"
        );
        assert_eq!(log.amount_wei_string(), "1000000000000000000");
    }

    fn indexed_address_topic(address: Address) -> B256 {
        let mut bytes = [0_u8; 32];
        bytes[12..].copy_from_slice(address.as_slice());
        B256::from(bytes)
    }

    fn encode_uint256(value: U256) -> Vec<u8> {
        DynSolValue::Uint(value, 256).abi_encode()
    }

    #[test]
    fn parses_exact_indexed_gateway_payment_log() {
        let expected = sample_payment_log();
        let log = Log {
            address: expected.gateway_address,
            topics: vec![
                payment_event_topic(),
                indexed_address_topic(expected.wallet),
                indexed_address_topic(expected.payer),
            ],
            data: encode_uint256(expected.amount_wei).into(),
            transaction_hash: Some(expected.tx_hash),
            log_index: Some(expected.log_index.into()),
            block_number: Some(expected.block_number.into()),
            ..Default::default()
        };

        let parsed = parse_gateway_payment_log(expected.chain_id, expected.gateway_address, log)
            .unwrap()
            .unwrap();

        assert_eq!(parsed, expected);
    }

    #[test]
    fn selects_matching_payment_log_from_submitted_tx_receipt() {
        let expected = sample_payment_log();
        let wrong_wallet = Address::from_str("0x9999999999999999999999999999999999999999").unwrap();
        let unrelated_transfer = Log {
            address: Address::from_str(LITKEY_TOKEN_ADDRESS).unwrap(),
            topics: vec![B256::ZERO],
            ..Default::default()
        };
        let wrong_wallet_payment = Log {
            address: expected.gateway_address,
            topics: vec![
                payment_event_topic(),
                indexed_address_topic(wrong_wallet),
                indexed_address_topic(expected.payer),
            ],
            data: encode_uint256(expected.amount_wei).into(),
            transaction_hash: Some(expected.tx_hash),
            log_index: Some((expected.log_index - 1).into()),
            block_number: Some(expected.block_number.into()),
            ..Default::default()
        };
        let matching_payment = Log {
            address: expected.gateway_address,
            topics: vec![
                payment_event_topic(),
                indexed_address_topic(expected.wallet),
                indexed_address_topic(expected.payer),
            ],
            data: encode_uint256(expected.amount_wei).into(),
            transaction_hash: Some(expected.tx_hash),
            log_index: Some(expected.log_index.into()),
            block_number: Some(expected.block_number.into()),
            ..Default::default()
        };

        let selected = select_matching_payment_log(
            expected.chain_id,
            expected.gateway_address,
            vec![unrelated_transfer, wrong_wallet_payment, matching_payment],
            expected.wallet,
        )
        .unwrap()
        .unwrap();

        assert_eq!(selected, expected);
    }

    #[test]
    fn rejects_malformed_matching_payment_logs_instead_of_skipping_them() {
        let expected = sample_payment_log();
        let mut log = Log {
            address: expected.gateway_address,
            topics: vec![
                payment_event_topic(),
                indexed_address_topic(expected.wallet),
            ],
            data: DynSolValue::Tuple(vec![
                DynSolValue::Address(expected.wallet),
                DynSolValue::Address(expected.payer),
                DynSolValue::Uint(expected.amount_wei, 256),
            ])
            .abi_encode()
            .into(),
            transaction_hash: Some(expected.tx_hash),
            log_index: Some(expected.log_index.into()),
            block_number: Some(expected.block_number.into()),
            ..Default::default()
        };

        let err =
            parse_gateway_payment_log(expected.chain_id, expected.gateway_address, log.clone())
                .unwrap_err();
        assert!(err.to_string().contains("exactly 3 topics"));

        log.topics = vec![
            payment_event_topic(),
            indexed_address_topic(expected.wallet),
            indexed_address_topic(expected.payer),
            B256::ZERO,
        ];
        let err = parse_gateway_payment_log(expected.chain_id, expected.gateway_address, log)
            .unwrap_err();
        assert!(err.to_string().contains("exactly 3 topics"));
    }

    #[test]
    fn rejects_payment_logs_from_the_wrong_gateway_address() {
        let expected = sample_payment_log();
        let other_gateway =
            Address::from_str("0x9999999999999999999999999999999999999999").unwrap();
        let log = Log {
            address: other_gateway,
            topics: vec![
                payment_event_topic(),
                indexed_address_topic(expected.wallet),
                indexed_address_topic(expected.payer),
            ],
            data: encode_uint256(expected.amount_wei).into(),
            transaction_hash: Some(expected.tx_hash),
            log_index: Some(expected.log_index.into()),
            block_number: Some(expected.block_number.into()),
            ..Default::default()
        };

        let err = parse_gateway_payment_log(expected.chain_id, expected.gateway_address, log)
            .unwrap_err();
        assert!(err.to_string().contains("did not match configured gateway"));
    }

    fn sample_payment_log() -> PaymentLog {
        PaymentLog {
            chain_id: 8453,
            gateway_address: Address::from_str("0x1000000000000000000000000000000000000000")
                .unwrap(),
            wallet: Address::from_str("0x2000000000000000000000000000000000000000").unwrap(),
            payer: Address::from_str("0x3000000000000000000000000000000000000000").unwrap(),
            amount_wei: U256::from_str("1000000000000000000").unwrap(),
            tx_hash: B256::from_str(
                "0x4444444444444444444444444444444444444444444444444444444444444444",
            )
            .unwrap(),
            log_index: 9,
            block_number: 1234,
        }
    }

    fn fresh_rate() -> crate::rate::LitkeyRate {
        crate::rate::LitkeyRate {
            usd_wei_per_litkey: "1000000000000000000".to_string(),
            source: "manual".to_string(),
            fetched_at: time::OffsetDateTime::now_utc(),
            updated_by_operator_id: Some(1),
            stale: false,
        }
    }

    #[test]
    fn classifies_missing_or_stale_rate_as_paused_without_credit_action() {
        let log = sample_payment_log();
        assert_eq!(
            classify_litkey_payment(&log, None, 0, Some("cus_123".to_string()))
                .unwrap()
                .status,
            PaymentStatus::Paused
        );

        let mut stale = fresh_rate();
        stale.stale = true;
        let decision =
            classify_litkey_payment(&log, Some(&stale), 0, Some("cus_123".to_string())).unwrap();
        assert_eq!(decision.status, PaymentStatus::Paused);
        assert!(decision.stripe_credit.is_none());
        assert_eq!(decision.payment.usd_wei_per_litkey, None);
    }

    #[test]
    fn classifies_zero_cent_payments_as_dust_without_credit_action() {
        let mut log = sample_payment_log();
        log.amount_wei = U256::from(1_u64);

        let decision =
            classify_litkey_payment(&log, Some(&fresh_rate()), 0, Some("cus_123".to_string()))
                .unwrap();

        assert_eq!(decision.status, PaymentStatus::Dust);
        assert_eq!(decision.payment.cents_credited, 0);
        assert!(decision.stripe_credit.is_none());
    }

    #[test]
    fn classifies_missing_customer_as_no_customer_without_credit_action() {
        let log = sample_payment_log();

        let decision = classify_litkey_payment(&log, Some(&fresh_rate()), 0, None).unwrap();

        assert_eq!(decision.status, PaymentStatus::NoCustomer);
        assert_eq!(decision.payment.cents_credited, 100);
        assert!(decision.stripe_credit.is_none());
    }

    #[test]
    fn classifies_customer_payment_as_credit_with_idempotency_and_audit_description() {
        let log = sample_payment_log();

        let decision = classify_litkey_payment(
            &log,
            Some(&fresh_rate()),
            2_000,
            Some("cus_123".to_string()),
        )
        .unwrap();
        let credit = decision.stripe_credit.expect("credit action");

        assert_eq!(decision.status, PaymentStatus::Credited);
        assert_eq!(decision.payment.cents_credited, 125);
        assert_eq!(credit.customer_id, "cus_123");
        assert_eq!(credit.amount_cents, -125);
        assert_eq!(credit.idempotency_key, log.idempotency_key());
        assert!(credit.description.contains("LITKEY payment"));
        assert!(
            credit
                .description
                .contains("0x4444444444444444444444444444444444444444444444444444444444444444")
        );
        assert!(credit.description.contains(&log.wallet_address()));
        assert!(credit.description.contains(&log.payer_address()));
        assert!(credit.description.contains("1000000000000000000 wei"));
        assert!(credit.description.contains("rate=1000000000000000000"));
        assert!(credit.description.contains("discount_bps=2000"));
    }
}
