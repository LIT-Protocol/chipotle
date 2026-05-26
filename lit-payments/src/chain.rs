//! LITKEY on-chain payment listener primitives.
//!
//! Phase 3c uses these helpers from both the WSS fast path and the HTTPS
//! reconciliation poller so confirmation depth, idempotency, and checkpoint
//! behavior cannot drift between paths.

use alloy_dyn_abi::DynSolType;
use alloy_primitives::{Address, B256, Bytes, U256, keccak256};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use lit_billing_core::{StripeClient, balance, customer};
use reqwest::Client;
use rocket::State;
use rocket::http::Status;
use rocket::serde::json::Json;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{collections::HashMap, time::Duration as StdDuration};
use tokio::time::{Duration, sleep, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};
pub const BASE_CHAIN_ID: i64 = 8453;
pub const LITKEY_TOKEN_ADDRESS: &str = "0xf732a566121fa6362e9e0fbdd6d66e5c8c925e49";
pub const DEFAULT_GATEWAY_ADDRESS: &str = "0xa2d54cd1d1df1735718a857ac49caf9ecab0093b";
pub const DEFAULT_CONFIRMATIONS: u64 = 5;
pub const DEFAULT_RECONCILIATION_INTERVAL_SECS: u64 = 60;
pub const MAX_RECONCILIATION_BLOCK_RANGE: u64 = 2_000;
const WSS_CONNECT_TIMEOUT_SECS: u64 = 15;
const WSS_SUBSCRIBE_ACK_TIMEOUT_SECS: u64 = 15;
const WSS_READ_IDLE_TIMEOUT_SECS: u64 = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainConfig {
    pub chain_id: i64,
    pub alchemy_wss_url: String,
    pub alchemy_https_url: String,
    pub gateway_address: Address,
    pub confirmations: u64,
    pub reconciliation_interval_secs: u64,
    pub reconciliation_start_block: u64,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WssPaymentNotification {
    Added(PaymentLog),
    Removed(PaymentLog),
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

pub fn is_confirmed(event_block: u64, latest_block: u64, confirmations: u64) -> bool {
    latest_block.saturating_sub(event_block) >= confirmations
}

pub fn reconciliation_range(
    last_processed_block: u64,
    latest_block: u64,
    confirmations: u64,
) -> Option<(u64, u64)> {
    let safe_to_block = latest_block.checked_sub(confirmations)?;
    let from_block = last_processed_block.saturating_add(1);
    (from_block <= safe_to_block).then_some((from_block, safe_to_block))
}

pub fn backoff_delay_secs(attempt: u32) -> u64 {
    2_u64.saturating_pow(attempt).clamp(1, 30)
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
            "LITKEY payments are not configured",
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

/// `GET /api/litkey/payment-status?tx_hash=…&wallet=…` — public status poller.
#[rocket::get("/api/litkey/payment-status?<tx_hash>&<wallet>")]
pub async fn get_payment_status(
    tx_hash: Option<&str>,
    wallet: Option<&str>,
    pool: &State<PgPool>,
) -> ApiResult<LitkeyPaymentStatusResponse> {
    let Some(tx_hash) = tx_hash else {
        return Err(api_err(Status::BadRequest, "tx_hash is required"));
    };
    let Some(wallet) = wallet else {
        return Err(api_err(Status::BadRequest, "wallet is required"));
    };
    let tx_hash = canonical_tx_hash_param(tx_hash).map_err(|e| api_err(Status::BadRequest, e))?;
    let wallet = canonical_wallet_param(wallet).map_err(|e| api_err(Status::BadRequest, e))?;
    let row = lookup_payment_status(pool, &tx_hash, &wallet)
        .await
        .map_err(api_server_err)?;
    Ok(Json(match row {
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
    }))
}

pub async fn current_checkpoint(
    pool: &PgPool,
    chain_id: i64,
    gateway_address: Address,
) -> Result<u64> {
    let block = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT last_processed_block FROM chain_checkpoint WHERE chain_id = $1 AND gateway_address = $2",
    )
    .bind(chain_id)
    .bind(format_address(gateway_address))
    .fetch_optional(pool)
    .await
    .context("reading chain checkpoint")?
    .flatten()
    .unwrap_or(0);
    u64::try_from(block).context("chain checkpoint was negative")
}

pub async fn advance_checkpoint(
    pool: &PgPool,
    chain_id: i64,
    gateway_address: Address,
    last_processed_block: u64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO chain_checkpoint (chain_id, gateway_address, last_processed_block)
         VALUES ($1, $2, $3)
         ON CONFLICT (chain_id, gateway_address) DO UPDATE SET
           last_processed_block = GREATEST(chain_checkpoint.last_processed_block, EXCLUDED.last_processed_block),
           updated_at = now()",
    )
    .bind(chain_id)
    .bind(format_address(gateway_address))
    .bind(last_processed_block as i64)
    .execute(pool)
    .await
    .context("advancing chain checkpoint")?;
    Ok(())
}

#[async_trait]
pub trait ChainRpc: Send + Sync {
    async fn latest_block(&self) -> Result<u64>;
    async fn payment_logs(&self, from_block: u64, to_block: u64) -> Result<Vec<PaymentLog>>;
}

#[async_trait]
pub trait ReconciliationStore: Send + Sync {
    async fn current_checkpoint(&self, chain_id: i64, gateway_address: Address) -> Result<u64>;
    async fn advance_checkpoint(
        &self,
        chain_id: i64,
        gateway_address: Address,
        last_processed_block: u64,
    ) -> Result<()>;
}

#[async_trait]
pub trait ConfirmedPaymentProcessor: Send + Sync {
    async fn process_confirmed_payment(&self, log: PaymentLog) -> Result<()>;
}

pub struct PgReconciliationStore {
    pool: PgPool,
}

impl PgReconciliationStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ReconciliationStore for PgReconciliationStore {
    async fn current_checkpoint(&self, chain_id: i64, gateway_address: Address) -> Result<u64> {
        current_checkpoint(&self.pool, chain_id, gateway_address).await
    }

    async fn advance_checkpoint(
        &self,
        chain_id: i64,
        gateway_address: Address,
        last_processed_block: u64,
    ) -> Result<()> {
        advance_checkpoint(&self.pool, chain_id, gateway_address, last_processed_block).await
    }
}

pub struct StripePaymentProcessor {
    pool: PgPool,
    stripe: StripeClient,
    discount_basis_points: i64,
}

impl StripePaymentProcessor {
    pub fn new(pool: PgPool, stripe: StripeClient, discount_basis_points: i64) -> Self {
        Self {
            pool,
            stripe,
            discount_basis_points,
        }
    }
}

#[async_trait]
impl ConfirmedPaymentProcessor for StripePaymentProcessor {
    async fn process_confirmed_payment(&self, log: PaymentLog) -> Result<()> {
        handle_confirmed_litkey_payment(&self.pool, &self.stripe, &log, self.discount_basis_points)
            .await
    }
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
    chain_id: i64,
    gateway_address: Address,
}

impl HttpGatewayRpc {
    pub fn new(config: &ChainConfig) -> Self {
        Self {
            client: rpc_http_client(),
            https_url: config.alchemy_https_url.clone(),
            chain_id: config.chain_id,
            gateway_address: config.gateway_address,
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

#[async_trait]
impl ChainRpc for HttpGatewayRpc {
    async fn latest_block(&self) -> Result<u64> {
        #[derive(Deserialize)]
        struct BlockNumber(#[serde(with = "alloy_serde::quantity")] u64);

        let block: BlockNumber = self.rpc("eth_blockNumber", json!([])).await?;
        Ok(block.0)
    }

    async fn payment_logs(&self, from_block: u64, to_block: u64) -> Result<Vec<PaymentLog>> {
        let logs: Vec<RpcLog> = self
            .rpc(
                "eth_getLogs",
                json!([{
                    "address": format_address(self.gateway_address),
                    "fromBlock": format!("0x{from_block:x}"),
                    "toBlock": format!("0x{to_block:x}"),
                    "topics": [format!("{:#x}", payment_event_topic())]
                }]),
            )
            .await?;
        logs.into_iter()
            .map(|log| parse_gateway_payment_log(self.chain_id, self.gateway_address, log.into()))
            .filter_map(|result| match result {
                Ok(Some(payment)) => Some(Ok(payment)),
                Ok(None) => None,
                Err(err) => Some(Err(err)),
            })
            .collect()
    }
}

pub const PAYMENT_EVENT_SIGNATURE: &str = "Payment(address,address,uint256)";

pub fn payment_event_topic() -> B256 {
    keccak256(PAYMENT_EVENT_SIGNATURE.as_bytes())
}

pub fn wss_payment_log_subscribe_request(gateway_address: Address) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_subscribe",
        "params": [
            "logs",
            {
                "address": format_address(gateway_address),
                "topics": [format!("{:#x}", payment_event_topic())]
            }
        ]
    })
}

pub fn parse_wss_subscription_ack(message: &str, request_id: u64) -> Result<Option<String>> {
    let value: Value = serde_json::from_str(message).context("decoding LITKEY WSS JSON message")?;
    if value.get("id").and_then(Value::as_u64) != Some(request_id) {
        return Ok(None);
    }

    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown JSON-RPC error");
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        anyhow::bail!("LITKEY WSS eth_subscribe failed: {message} ({code})");
    }

    let subscription_id = value
        .get("result")
        .and_then(Value::as_str)
        .context("LITKEY WSS eth_subscribe response missing subscription id")?;
    Ok(Some(subscription_id.to_string()))
}

fn wss_notification_result(message: &str, subscription_id: &str) -> Result<Option<Value>> {
    let value: Value = serde_json::from_str(message).context("decoding LITKEY WSS JSON message")?;
    if value.get("method").and_then(Value::as_str) != Some("eth_subscription") {
        return Ok(None);
    }

    let params = value
        .get("params")
        .context("LITKEY WSS subscription message missing params")?;
    let actual_subscription = params
        .get("subscription")
        .and_then(Value::as_str)
        .context("LITKEY WSS subscription message missing subscription id")?;
    if actual_subscription != subscription_id {
        return Ok(None);
    }

    Ok(Some(params.get("result").cloned().context(
        "LITKEY WSS subscription message missing result",
    )?))
}

pub fn parse_wss_payment_log_notification(
    chain_id: i64,
    gateway_address: Address,
    subscription_id: &str,
    message: &str,
) -> Result<Option<WssPaymentNotification>> {
    let Some(result) = wss_notification_result(message, subscription_id)? else {
        return Ok(None);
    };

    let removed = result
        .get("removed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let rpc_log: RpcLog =
        serde_json::from_value(result).context("decoding LITKEY WSS payment log")?;
    let Some(payment_log) = parse_gateway_payment_log(chain_id, gateway_address, rpc_log.into())?
    else {
        return Ok(None);
    };
    if removed {
        tracing::warn!(
            tx_hash = %format!("{:#x}", payment_log.tx_hash),
            log_index = payment_log.log_index,
            block_number = payment_log.block_number,
            "LITKEY WSS received removed/reorged payment log"
        );
        return Ok(Some(WssPaymentNotification::Removed(payment_log)));
    }
    Ok(Some(WssPaymentNotification::Added(payment_log)))
}

fn pending_payment_key(log: &PaymentLog) -> String {
    log.idempotency_key()
}

pub async fn drain_confirmed_wss_payments<R, P>(
    rpc: &R,
    processor: &P,
    config: &ChainConfig,
    pending: &mut HashMap<String, PaymentLog>,
) -> Result<usize>
where
    R: ChainRpc,
    P: ConfirmedPaymentProcessor,
{
    if pending.is_empty() {
        return Ok(0);
    }

    let latest_block = rpc.latest_block().await?;
    let mut confirmed_keys = Vec::new();
    for (key, log) in pending.iter() {
        if is_confirmed(log.block_number, latest_block, config.confirmations) {
            confirmed_keys.push(key.clone());
        }
    }
    confirmed_keys.sort();

    let mut processed = 0;
    for key in confirmed_keys {
        let Some(log) = pending.get(&key).cloned() else {
            continue;
        };
        processor.process_confirmed_payment(log).await?;
        pending.remove(&key);
        processed += 1;
    }
    Ok(processed)
}

pub async fn process_wss_payment_notification<R, P>(
    rpc: &R,
    processor: &P,
    config: &ChainConfig,
    subscription_id: &str,
    message: &str,
    pending: &mut HashMap<String, PaymentLog>,
) -> Result<bool>
where
    R: ChainRpc,
    P: ConfirmedPaymentProcessor,
{
    let Some(notification) = parse_wss_payment_log_notification(
        config.chain_id,
        config.gateway_address,
        subscription_id,
        message,
    )?
    else {
        return Ok(false);
    };

    let log = match notification {
        WssPaymentNotification::Added(log) => log,
        WssPaymentNotification::Removed(log) => {
            pending.remove(&pending_payment_key(&log));
            return Ok(false);
        }
    };

    let latest_block = rpc.latest_block().await?;
    if is_confirmed(log.block_number, latest_block, config.confirmations) {
        processor.process_confirmed_payment(log).await?;
        return Ok(true);
    }

    let key = pending_payment_key(&log);
    tracing::debug!(
        tx_hash = %format!("{:#x}", log.tx_hash),
        log_index = log.log_index,
        block_number = log.block_number,
        latest_block,
        confirmations = config.confirmations,
        "LITKEY WSS payment log is pending confirmations"
    );
    pending.insert(key, log);
    Ok(false)
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
    let mut decision =
        classify_litkey_payment(log, rate.as_ref(), discount_basis_points, customer_id)?;

    if let Some(credit) = &decision.stripe_credit {
        let balance_transaction_id = balance::write_transaction(
            stripe,
            &credit.customer_id,
            credit.amount_cents,
            &credit.description,
            Some(&credit.idempotency_key),
        )
        .await?;
        decision.payment.stripe_balance_transaction_id = Some(balance_transaction_id);
    }

    insert_payment(pool, &decision.payment).await?;
    Ok(())
}

pub async fn process_reconciliation_once<S, R, P>(
    store: &S,
    rpc: &R,
    processor: &P,
    config: &ChainConfig,
) -> Result<()>
where
    S: ReconciliationStore,
    R: ChainRpc,
    P: ConfirmedPaymentProcessor,
{
    let stored_checkpoint = store
        .current_checkpoint(config.chain_id, config.gateway_address)
        .await?;
    let checkpoint = stored_checkpoint.max(config.reconciliation_start_block.saturating_sub(1));
    let latest_block = rpc.latest_block().await?;
    let Some((from_block, to_block)) =
        reconciliation_range(checkpoint, latest_block, config.confirmations)
    else {
        return Ok(());
    };

    let chunk_to_block = to_block.min(
        from_block
            .saturating_add(MAX_RECONCILIATION_BLOCK_RANGE)
            .saturating_sub(1),
    );
    tracing::debug!(
        stored_checkpoint,
        effective_checkpoint = checkpoint,
        from_block,
        to_block = chunk_to_block,
        latest_block,
        confirmations = config.confirmations,
        "LITKEY reconciliation scanning confirmed range"
    );
    let mut logs = rpc.payment_logs(from_block, chunk_to_block).await?;
    logs.sort_by_key(|log| (log.block_number, log.log_index));
    for log in logs {
        processor.process_confirmed_payment(log).await?;
    }
    store
        .advance_checkpoint(config.chain_id, config.gateway_address, chunk_to_block)
        .await?;
    Ok(())
}

pub async fn reconciliation_loop<S, R, P>(store: S, rpc: R, processor: P, config: ChainConfig)
where
    S: ReconciliationStore,
    R: ChainRpc,
    P: ConfirmedPaymentProcessor,
{
    let mut attempt = 0_u32;
    loop {
        match process_reconciliation_once(&store, &rpc, &processor, &config).await {
            Ok(()) => {
                attempt = 0;
                sleep(Duration::from_secs(config.reconciliation_interval_secs)).await;
            }
            Err(err) => {
                tracing::warn!(error = %err, "LITKEY reconciliation pass failed");
                let delay = backoff_delay_secs(attempt);
                attempt = attempt.saturating_add(1);
                sleep(Duration::from_secs(delay)).await;
            }
        }
    }
}

async fn next_wss_text<S>(socket: &mut S) -> Result<String>
where
    S: StreamExt<Item = std::result::Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    let message = timeout(
        Duration::from_secs(WSS_READ_IDLE_TIMEOUT_SECS),
        socket.next(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("LITKEY WSS read timed out"))?
    .context("LITKEY WSS stream ended")?
    .context("reading LITKEY WSS message")?;

    match message {
        Message::Text(text) => Ok(text.to_string()),
        Message::Binary(bytes) => std::str::from_utf8(&bytes)
            .context("decoding LITKEY WSS binary JSON")
            .map(str::to_owned),
        Message::Close(frame) => anyhow::bail!("LITKEY WSS closed: {frame:?}"),
        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => Ok(String::new()),
    }
}

async fn wait_for_wss_subscription_ack<S>(socket: &mut S, request_id: u64) -> Result<String>
where
    S: StreamExt<Item = std::result::Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    timeout(Duration::from_secs(WSS_SUBSCRIBE_ACK_TIMEOUT_SECS), async {
        loop {
            let text = next_wss_text(socket).await?;
            if text.is_empty() {
                continue;
            }
            if let Some(subscription_id) = parse_wss_subscription_ack(&text, request_id)? {
                return Ok(subscription_id);
            }
        }
    })
    .await
    .context("timed out waiting for LITKEY WSS subscription ack")?
}

async fn wss_listener_connection_once<P>(processor: &P, config: &ChainConfig) -> Result<()>
where
    P: ConfirmedPaymentProcessor,
{
    let latest_rpc = HttpGatewayRpc::new(config);
    let (mut socket, _) = timeout(
        Duration::from_secs(WSS_CONNECT_TIMEOUT_SECS),
        connect_async(&config.alchemy_wss_url),
    )
    .await
    .context("timed out connecting LITKEY Alchemy WSS")?
    .context("connecting LITKEY Alchemy WSS")?;

    socket
        .send(Message::Text(
            wss_payment_log_subscribe_request(config.gateway_address)
                .to_string()
                .into(),
        ))
        .await
        .context("subscribing to LITKEY Payment logs over WSS")?;

    let subscription_id = wait_for_wss_subscription_ack(&mut socket, 1).await?;
    tracing::info!(subscription_id, "LITKEY WSS subscription acknowledged");

    let mut pending = HashMap::new();
    loop {
        let text = next_wss_text(&mut socket).await?;
        if text.is_empty() {
            drain_confirmed_wss_payments(&latest_rpc, processor, config, &mut pending).await?;
            continue;
        }
        process_wss_payment_notification(
            &latest_rpc,
            processor,
            config,
            &subscription_id,
            &text,
            &mut pending,
        )
        .await?;
        drain_confirmed_wss_payments(&latest_rpc, processor, config, &mut pending).await?;
    }
}

pub async fn wss_listener_loop<P>(processor: P, config: ChainConfig)
where
    P: ConfirmedPaymentProcessor,
{
    let mut attempt = 0_u32;
    loop {
        match wss_listener_connection_once(&processor, &config).await {
            Ok(()) => attempt = 0,
            Err(err) => {
                tracing::warn!(error = %err, "LITKEY WSS listener connection failed");
                let delay = backoff_delay_secs(attempt);
                attempt = attempt.saturating_add(1);
                sleep(Duration::from_secs(delay)).await;
            }
        }
    }
}

/// Register the phase-3c LITKEY listener when chain config is present.
///
/// Starts both the Alchemy WSS fast path and the confirmed-block HTTPS
/// reconciliation poller. Both paths share the same parser, handler, and
/// idempotency key; WSS never advances reconciliation checkpoints.
pub fn spawn_litkey_listener(
    pool: PgPool,
    stripe: StripeClient,
    config: Option<ChainConfig>,
    discount_basis_points: i64,
) {
    let Some(config) = config else {
        tracing::info!("LITKEY listener disabled; ALCHEMY_* and LITKEY_GATEWAY_ADDRESS not set");
        return;
    };

    let rpc = HttpGatewayRpc::new(&config);
    let store = PgReconciliationStore::new(pool.clone());
    let reconciliation_processor =
        StripePaymentProcessor::new(pool.clone(), stripe.clone(), discount_basis_points);
    let task_config = config.clone();
    tokio::spawn(async move {
        reconciliation_loop(store, rpc, reconciliation_processor, task_config).await;
    });

    let wss_processor = StripePaymentProcessor::new(pool, stripe, discount_basis_points);
    let task_config = config.clone();
    tokio::spawn(async move {
        wss_listener_loop(wss_processor, task_config).await;
    });

    tracing::info!(
        chain_id = config.chain_id,
        gateway_address = %format_address(config.gateway_address),
        confirmations = config.confirmations,
        reconciliation_interval_secs = config.reconciliation_interval_secs,
        reconciliation_start_block = config.reconciliation_start_block,
        discount_basis_points,
        "LITKEY listener started with Alchemy WSS fast path and HTTPS reconciliation fallback"
    );
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
    fn confirmation_depth_is_inclusive() {
        assert!(is_confirmed(100, 105, 5));
        assert!(!is_confirmed(100, 104, 5));
        assert!(is_confirmed(100, 100, 0));
    }

    #[test]
    fn reconciliation_range_starts_after_checkpoint_and_stops_at_safe_block() {
        assert_eq!(reconciliation_range(100, 110, 5), Some((101, 105)));
        assert_eq!(reconciliation_range(100, 105, 5), None);
        assert_eq!(reconciliation_range(0, 3, 5), None);
        assert_eq!(reconciliation_range(0, 5, 5), None);
        assert_eq!(reconciliation_range(0, 6, 5), Some((1, 1)));
    }

    #[test]
    fn reconnect_backoff_caps_at_thirty_seconds() {
        let delays: Vec<u64> = (0..8).map(backoff_delay_secs).collect();
        assert_eq!(delays, vec![1, 2, 4, 8, 16, 30, 30, 30]);
    }

    #[test]
    fn builds_alchemy_wss_payment_log_subscription_for_configured_gateway() {
        let config = sample_chain_config();
        let payload = wss_payment_log_subscribe_request(config.gateway_address);

        assert_eq!(payload["jsonrpc"], "2.0");
        assert_eq!(payload["id"], 1);
        assert_eq!(payload["method"], "eth_subscribe");
        assert_eq!(payload["params"][0], "logs");
        assert_eq!(
            payload["params"][1]["address"],
            format_address(config.gateway_address)
        );
        assert_eq!(
            payload["params"][1]["topics"][0],
            format!("{:#x}", payment_event_topic())
        );
    }

    #[test]
    fn parses_wss_subscription_ack_and_errors() {
        assert_eq!(
            parse_wss_subscription_ack(r#"{"jsonrpc":"2.0","id":1,"result":"0xsub"}"#, 1).unwrap(),
            Some("0xsub".to_string())
        );
        assert!(
            parse_wss_subscription_ack(r#"{"jsonrpc":"2.0","id":2,"result":"0xother"}"#, 1)
                .unwrap()
                .is_none()
        );
        let err = parse_wss_subscription_ack(
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"nope"}}"#,
            1,
        )
        .unwrap_err();
        assert!(err.to_string().contains("eth_subscribe failed"));
    }

    #[test]
    fn parses_eth_subscription_payment_log_notification_with_existing_parser() {
        let expected = sample_payment_log();
        let message = sample_subscription_message(&expected);

        let parsed = parse_wss_payment_log_notification(
            expected.chain_id,
            expected.gateway_address,
            "0xsub",
            &message.to_string(),
        )
        .unwrap()
        .unwrap();

        assert_eq!(parsed, WssPaymentNotification::Added(expected));
    }

    #[test]
    fn ignores_unrelated_wss_messages_but_errors_on_malformed_subscribed_logs() {
        let expected = sample_payment_log();
        assert!(
            parse_wss_payment_log_notification(
                expected.chain_id,
                expected.gateway_address,
                "0xsub",
                r#"{"jsonrpc":"2.0","id":1,"result":"0xsub"}"#,
            )
            .unwrap()
            .is_none()
        );
        assert!(
            parse_wss_payment_log_notification(
                expected.chain_id,
                expected.gateway_address,
                "0xsub",
                r#"{"jsonrpc":"2.0","method":"eth_subscription","params":{"subscription":"0xother","result":{}}}"#,
            )
            .unwrap()
            .is_none()
        );

        let missing_topics = json!({
            "jsonrpc": "2.0",
            "method": "eth_subscription",
            "params": {
                "subscription": "0xsub",
                "result": {
                    "address": format_address(expected.gateway_address),
                    "blockNumber": format!("0x{:x}", expected.block_number),
                    "logIndex": format!("0x{:x}", expected.log_index),
                    "transactionHash": format!("{:#x}", expected.tx_hash),
                    "data": format!("0x{}", hex::encode(encode_uint256(expected.amount_wei)))
                }
            }
        });
        let err = parse_wss_payment_log_notification(
            expected.chain_id,
            expected.gateway_address,
            "0xsub",
            &missing_topics.to_string(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("decoding LITKEY WSS payment log"));

        let malformed = json!({
            "jsonrpc": "2.0",
            "method": "eth_subscription",
            "params": {
                "subscription": "0xsub",
                "result": {
                    "address": format_address(expected.gateway_address),
                    "blockNumber": format!("0x{:x}", expected.block_number),
                    "logIndex": format!("0x{:x}", expected.log_index),
                    "transactionHash": format!("{:#x}", expected.tx_hash),
                    "topics": [format!("{:#x}", payment_event_topic()), indexed_address_topic(expected.wallet)],
                    "data": format!("0x{}", hex::encode(encode_uint256(expected.amount_wei)))
                }
            }
        });

        let err = parse_wss_payment_log_notification(
            expected.chain_id,
            expected.gateway_address,
            "0xsub",
            &malformed.to_string(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("exactly 3 topics"));
    }

    #[test]
    fn parses_removed_wss_logs_for_pending_eviction() {
        let expected = sample_payment_log();
        let mut message = sample_subscription_message(&expected);
        message["params"]["result"]["removed"] = json!(true);

        assert_eq!(
            parse_wss_payment_log_notification(
                expected.chain_id,
                expected.gateway_address,
                "0xsub",
                &message.to_string(),
            )
            .unwrap(),
            Some(WssPaymentNotification::Removed(expected))
        );
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

    fn sample_subscription_message(log: &PaymentLog) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "eth_subscription",
            "params": {
                "subscription": "0xsub",
                "result": {
                    "address": format_address(log.gateway_address),
                    "blockNumber": format!("0x{:x}", log.block_number),
                    "logIndex": format!("0x{:x}", log.log_index),
                    "transactionHash": format!("{:#x}", log.tx_hash),
                    "topics": [
                        format!("{:#x}", payment_event_topic()),
                        format!("{:#x}", indexed_address_topic(log.wallet)),
                        format!("{:#x}", indexed_address_topic(log.payer))
                    ],
                    "data": format!("0x{}", hex::encode(encode_uint256(log.amount_wei)))
                }
            }
        })
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

    struct FakeStore {
        checkpoint: u64,
        advanced_to: std::sync::Mutex<Vec<u64>>,
    }

    #[async_trait]
    impl ReconciliationStore for FakeStore {
        async fn current_checkpoint(
            &self,
            _chain_id: i64,
            _gateway_address: Address,
        ) -> Result<u64> {
            Ok(self.checkpoint)
        }

        async fn advance_checkpoint(
            &self,
            _chain_id: i64,
            _gateway_address: Address,
            last_processed_block: u64,
        ) -> Result<()> {
            self.advanced_to.lock().unwrap().push(last_processed_block);
            Ok(())
        }
    }

    struct FakeRpc {
        latest: u64,
        logs: Vec<PaymentLog>,
        ranges: std::sync::Mutex<Vec<(u64, u64)>>,
    }

    #[async_trait]
    impl ChainRpc for FakeRpc {
        async fn latest_block(&self) -> Result<u64> {
            Ok(self.latest)
        }

        async fn payment_logs(&self, from_block: u64, to_block: u64) -> Result<Vec<PaymentLog>> {
            self.ranges.lock().unwrap().push((from_block, to_block));
            Ok(self.logs.clone())
        }
    }

    struct FakeProcessor {
        processed: std::sync::Mutex<Vec<(u64, u64)>>,
        fail: bool,
    }

    #[async_trait]
    impl ConfirmedPaymentProcessor for FakeProcessor {
        async fn process_confirmed_payment(&self, log: PaymentLog) -> Result<()> {
            if self.fail {
                anyhow::bail!("boom");
            }
            self.processed
                .lock()
                .unwrap()
                .push((log.block_number, log.log_index));
            Ok(())
        }
    }

    fn sample_chain_config() -> ChainConfig {
        ChainConfig {
            chain_id: 8453,
            alchemy_wss_url: "wss://example.invalid".to_string(),
            alchemy_https_url: "https://example.invalid".to_string(),
            gateway_address: Address::from_str("0x1000000000000000000000000000000000000000")
                .unwrap(),
            confirmations: 5,
            reconciliation_interval_secs: 60,
            reconciliation_start_block: 0,
        }
    }

    #[tokio::test]
    async fn wss_notification_processes_only_confirmed_payment_logs() {
        let log = sample_payment_log();
        let message = sample_subscription_message(&log).to_string();
        let mut config = sample_chain_config();
        config.gateway_address = log.gateway_address;

        let rpc = FakeRpc {
            latest: log.block_number + config.confirmations,
            logs: vec![],
            ranges: std::sync::Mutex::new(Vec::new()),
        };
        let processor = FakeProcessor {
            processed: std::sync::Mutex::new(Vec::new()),
            fail: false,
        };
        let mut pending = HashMap::new();

        assert!(
            process_wss_payment_notification(
                &rpc,
                &processor,
                &config,
                "0xsub",
                &message,
                &mut pending,
            )
            .await
            .unwrap()
        );
        assert_eq!(
            *processor.processed.lock().unwrap(),
            vec![(log.block_number, log.log_index)]
        );
        assert!(pending.is_empty());
        assert!(rpc.ranges.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn wss_notification_buffers_unconfirmed_logs_and_drains_when_confirmed() {
        let log = sample_payment_log();
        let message = sample_subscription_message(&log).to_string();
        let mut config = sample_chain_config();
        config.gateway_address = log.gateway_address;

        let unconfirmed_rpc = FakeRpc {
            latest: log.block_number + config.confirmations - 1,
            logs: vec![],
            ranges: std::sync::Mutex::new(Vec::new()),
        };
        let processor = FakeProcessor {
            processed: std::sync::Mutex::new(Vec::new()),
            fail: false,
        };
        let mut pending = HashMap::new();

        assert!(
            !process_wss_payment_notification(
                &unconfirmed_rpc,
                &processor,
                &config,
                "0xsub",
                &message,
                &mut pending,
            )
            .await
            .unwrap()
        );
        assert!(processor.processed.lock().unwrap().is_empty());
        assert_eq!(pending.len(), 1);
        assert!(unconfirmed_rpc.ranges.lock().unwrap().is_empty());

        let confirmed_rpc = FakeRpc {
            latest: log.block_number + config.confirmations,
            logs: vec![],
            ranges: std::sync::Mutex::new(Vec::new()),
        };
        assert_eq!(
            drain_confirmed_wss_payments(&confirmed_rpc, &processor, &config, &mut pending)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            *processor.processed.lock().unwrap(),
            vec![(log.block_number, log.log_index)]
        );
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn wss_removed_notification_evicts_pending_log_before_drain() {
        let log = sample_payment_log();
        let message = sample_subscription_message(&log).to_string();
        let mut removed_message = sample_subscription_message(&log);
        removed_message["params"]["result"]["removed"] = json!(true);
        let mut config = sample_chain_config();
        config.gateway_address = log.gateway_address;

        let unconfirmed_rpc = FakeRpc {
            latest: log.block_number + config.confirmations - 1,
            logs: vec![],
            ranges: std::sync::Mutex::new(Vec::new()),
        };
        let processor = FakeProcessor {
            processed: std::sync::Mutex::new(Vec::new()),
            fail: false,
        };
        let mut pending = HashMap::new();

        process_wss_payment_notification(
            &unconfirmed_rpc,
            &processor,
            &config,
            "0xsub",
            &message,
            &mut pending,
        )
        .await
        .unwrap();
        assert_eq!(pending.len(), 1);

        process_wss_payment_notification(
            &unconfirmed_rpc,
            &processor,
            &config,
            "0xsub",
            &removed_message.to_string(),
            &mut pending,
        )
        .await
        .unwrap();
        assert!(pending.is_empty());

        let confirmed_rpc = FakeRpc {
            latest: log.block_number + config.confirmations,
            logs: vec![],
            ranges: std::sync::Mutex::new(Vec::new()),
        };
        assert_eq!(
            drain_confirmed_wss_payments(&confirmed_rpc, &processor, &config, &mut pending)
                .await
                .unwrap(),
            0
        );
        assert!(processor.processed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reconciliation_once_processes_safe_range_and_advances_checkpoint_after_success() {
        let mut later = sample_payment_log();
        later.block_number = 105;
        later.log_index = 2;
        let mut earlier = sample_payment_log();
        earlier.block_number = 101;
        earlier.log_index = 1;
        let store = FakeStore {
            checkpoint: 100,
            advanced_to: std::sync::Mutex::new(Vec::new()),
        };
        let rpc = FakeRpc {
            latest: 110,
            logs: vec![later, earlier],
            ranges: std::sync::Mutex::new(Vec::new()),
        };
        let processor = FakeProcessor {
            processed: std::sync::Mutex::new(Vec::new()),
            fail: false,
        };

        process_reconciliation_once(&store, &rpc, &processor, &sample_chain_config())
            .await
            .unwrap();

        assert_eq!(*rpc.ranges.lock().unwrap(), vec![(101, 105)]);
        assert_eq!(
            *processor.processed.lock().unwrap(),
            vec![(101, 1), (105, 2)]
        );
        assert_eq!(*store.advanced_to.lock().unwrap(), vec![105]);
    }

    #[tokio::test]
    async fn reconciliation_once_chunks_large_catchup_ranges() {
        let store = FakeStore {
            checkpoint: 0,
            advanced_to: std::sync::Mutex::new(Vec::new()),
        };
        let rpc = FakeRpc {
            latest: MAX_RECONCILIATION_BLOCK_RANGE + 10,
            logs: vec![],
            ranges: std::sync::Mutex::new(Vec::new()),
        };
        let processor = FakeProcessor {
            processed: std::sync::Mutex::new(Vec::new()),
            fail: false,
        };

        process_reconciliation_once(&store, &rpc, &processor, &sample_chain_config())
            .await
            .unwrap();

        assert_eq!(
            *rpc.ranges.lock().unwrap(),
            vec![(1, MAX_RECONCILIATION_BLOCK_RANGE)]
        );
        assert_eq!(
            *store.advanced_to.lock().unwrap(),
            vec![MAX_RECONCILIATION_BLOCK_RANGE]
        );
    }

    #[tokio::test]
    async fn reconciliation_once_uses_configured_start_block_when_checkpoint_is_behind() {
        let store = FakeStore {
            checkpoint: 2_000,
            advanced_to: std::sync::Mutex::new(Vec::new()),
        };
        let rpc = FakeRpc {
            latest: 46_516_500,
            logs: vec![],
            ranges: std::sync::Mutex::new(Vec::new()),
        };
        let processor = FakeProcessor {
            processed: std::sync::Mutex::new(Vec::new()),
            fail: false,
        };
        let mut config = sample_chain_config();
        config.reconciliation_start_block = 46_516_000;

        process_reconciliation_once(&store, &rpc, &processor, &config)
            .await
            .unwrap();

        assert_eq!(*rpc.ranges.lock().unwrap(), vec![(46_516_000, 46_516_495)]);
        assert_eq!(*store.advanced_to.lock().unwrap(), vec![46_516_495]);
    }

    #[tokio::test]
    async fn reconciliation_once_does_not_advance_checkpoint_when_processing_fails() {
        let store = FakeStore {
            checkpoint: 100,
            advanced_to: std::sync::Mutex::new(Vec::new()),
        };
        let rpc = FakeRpc {
            latest: 110,
            logs: vec![sample_payment_log()],
            ranges: std::sync::Mutex::new(Vec::new()),
        };
        let processor = FakeProcessor {
            processed: std::sync::Mutex::new(Vec::new()),
            fail: true,
        };

        let err = process_reconciliation_once(&store, &rpc, &processor, &sample_chain_config())
            .await
            .unwrap_err();

        assert!(err.to_string().contains("boom"));
        assert!(store.advanced_to.lock().unwrap().is_empty());
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
