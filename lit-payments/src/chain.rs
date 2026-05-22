//! LITKEY on-chain payment listener primitives.
//!
//! Phase 3c uses these helpers from both the WSS fast path and the HTTPS
//! reconciliation poller so confirmation depth, idempotency, and checkpoint
//! behavior cannot drift between paths.

use anyhow::{Context, Result};
use async_trait::async_trait;
use ethers_core::abi::{ParamType, decode};
use ethers_core::types::{Address, Bytes, H256, Log, U64, U256};
use ethers_core::utils::keccak256;
use lit_billing_core::{StripeClient, balance, customer};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use std::time::Duration as StdDuration;
use tokio::time::{Duration, sleep};
pub const BASE_CHAIN_ID: i64 = 8453;
pub const DEFAULT_CONFIRMATIONS: u64 = 5;
pub const DEFAULT_RECONCILIATION_INTERVAL_SECS: u64 = 60;
pub const MAX_RECONCILIATION_BLOCK_RANGE: u64 = 2_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainConfig {
    pub chain_id: i64,
    pub alchemy_wss_url: String,
    pub alchemy_https_url: String,
    pub gateway_address: Address,
    pub confirmations: u64,
    pub reconciliation_interval_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentLog {
    pub chain_id: i64,
    pub gateway_address: Address,
    pub wallet: Address,
    pub payer: Address,
    pub amount_wei: U256,
    pub tx_hash: H256,
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

pub fn payment_idempotency_key(chain_id: i64, tx_hash: H256, log_index: u64) -> String {
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
    block_number: Option<U64>,
    #[serde(rename = "logIndex")]
    log_index: Option<U256>,
    #[serde(rename = "transactionHash")]
    transaction_hash: Option<H256>,
    topics: Vec<H256>,
    data: Bytes,
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
            ..Default::default()
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
        let block: U64 = self.rpc("eth_blockNumber", json!([])).await?;
        Ok(block.as_u64())
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

pub fn payment_event_topic() -> H256 {
    H256::from(keccak256(PAYMENT_EVENT_SIGNATURE.as_bytes()))
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
    let log_index = log
        .log_index
        .context("payment log missing log index")?
        .as_u64();
    let block_number = log
        .block_number
        .context("payment log missing block number")?
        .as_u64();

    let wallet = Address::from_slice(&log.topics[1].as_bytes()[12..]);
    let payer = Address::from_slice(&log.topics[2].as_bytes()[12..]);
    let decoded =
        decode(&[ParamType::Uint(256)], &log.data.0).context("decoding indexed Payment amount")?;
    let amount_wei = decoded[0]
        .clone()
        .into_uint()
        .context("Payment amount not uint")?;

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
    let checkpoint = store
        .current_checkpoint(config.chain_id, config.gateway_address)
        .await?;
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

/// Register the phase-3c LITKEY listener when chain config is present.
///
/// This slice starts the confirmed-block HTTPS reconciliation poller. The WSS
/// subscription fast path remains intentionally deferred; both paths will share
/// the same handler and idempotency key when WSS is added.
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
    let processor = StripePaymentProcessor::new(pool, stripe, discount_basis_points);
    let task_config = config.clone();
    tokio::spawn(async move {
        reconciliation_loop(store, rpc, processor, task_config).await;
    });

    tracing::info!(
        chain_id = config.chain_id,
        gateway_address = %format_address(config.gateway_address),
        confirmations = config.confirmations,
        reconciliation_interval_secs = config.reconciliation_interval_secs,
        discount_basis_points,
        "LITKEY reconciliation listener started; WSS subscription remains deferred"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers_core::abi::{Token, encode};
    use ethers_core::types::{Address, H256, Log, U256};
    use std::str::FromStr;

    #[test]
    fn formats_deterministic_stripe_idempotency_key() {
        let tx_hash =
            H256::from_str("0x1111111111111111111111111111111111111111111111111111111111111111")
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
    fn payment_log_normalizes_addresses_and_amounts_for_processing() {
        let wallet = Address::from_str("0xA2D54CD1D1dF1735718A857aC49CaF9ECaB0093b").unwrap();
        let payer = Address::from_str("0x000000000000000000000000000000000000dEaD").unwrap();
        let log = PaymentLog {
            chain_id: 8453,
            gateway_address: wallet,
            wallet,
            payer,
            amount_wei: U256::from_dec_str("1000000000000000000").unwrap(),
            tx_hash: H256::zero(),
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

    fn indexed_address_topic(address: Address) -> H256 {
        let mut bytes = [0_u8; 32];
        bytes[12..].copy_from_slice(address.as_bytes());
        H256::from(bytes)
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
            data: encode(&[Token::Uint(expected.amount_wei)]).into(),
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
            data: encode(&[
                Token::Address(expected.wallet),
                Token::Address(expected.payer),
                Token::Uint(expected.amount_wei),
            ])
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
            H256::zero(),
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
            data: encode(&[Token::Uint(expected.amount_wei)]).into(),
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
            amount_wei: U256::from_dec_str("1000000000000000000").unwrap(),
            tx_hash: H256::from_str(
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
        }
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
