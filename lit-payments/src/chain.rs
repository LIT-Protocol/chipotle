//! LITKEY on-chain payment listener primitives.
//!
//! Phase 3c uses these helpers from both the WSS fast path and the HTTPS
//! reconciliation poller so confirmation depth, idempotency, and checkpoint
//! behavior cannot drift between paths.

use anyhow::{Context, Result};
use ethers_core::types::{Address, H256, U256};
use lit_billing_core::StripeClient;
use sqlx::PgPool;
pub const BASE_CHAIN_ID: i64 = 8453;
pub const DEFAULT_CONFIRMATIONS: u64 = 5;
pub const DEFAULT_RECONCILIATION_INTERVAL_SECS: u64 = 60;

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

/// Register the phase-3c LITKEY listener scaffold when chain config is present.
///
/// This does not yet subscribe to RPC. It validates and logs the configured
/// gateway so the DB/idempotency/checkpoint pieces can ship before live
/// processing; live WSS/poller tasks must be added before production crediting.
pub fn spawn_litkey_listener(
    _pool: PgPool,
    _stripe: StripeClient,
    config: Option<ChainConfig>,
    discount_basis_points: i64,
) {
    let Some(config) = config else {
        tracing::info!("LITKEY listener disabled; ALCHEMY_* and LITKEY_GATEWAY_ADDRESS not set");
        return;
    };

    tracing::info!(
        chain_id = config.chain_id,
        gateway_address = %format_address(config.gateway_address),
        confirmations = config.confirmations,
        reconciliation_interval_secs = config.reconciliation_interval_secs,
        discount_basis_points,
        "LITKEY listener scaffold configured; live WSS/reconciliation tasks are still disabled"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers_core::types::{Address, H256, U256};
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
}
