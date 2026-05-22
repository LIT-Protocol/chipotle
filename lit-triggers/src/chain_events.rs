//! EVM chain-event trigger polling listener.
//!
//! Phase 4 intentionally implements the durable polling path first. WebSocket
//! endpoint names are present in chain specs/config for a follow-up listener,
//! but this module only uses JSON-RPC HTTP polling.

use std::collections::BTreeMap;
use std::time::Duration;

use alloy_primitives::keccak256;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::config::{chain_spec_by_id, chain_spec_by_key, ChainSpec, Config};

const CHAIN_EVENTS_LOCK_KEY: i64 = 0x6c69_745f_6368_6169; // "lit_chai" prefix, arbitrary app lock.

#[derive(Clone, Debug)]
struct ActiveChain {
    spec: &'static ChainSpec,
    rpc_url: String,
}

#[derive(Debug)]
struct ChainEventTrigger {
    id: Uuid,
    config: Value,
    max_queued_runs: Option<i32>,
    last_block: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedChainEventConfig {
    pub chain_key: &'static str,
    pub chain_id: u64,
    pub contract_address: String,
    pub event_signature: String,
    pub topic0: String,
    pub topic_filters: Vec<Value>,
    pub start_block: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Debug, Clone, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EvmLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    #[serde(rename = "blockNumber")]
    pub block_number: String,
    #[serde(rename = "transactionHash")]
    pub transaction_hash: String,
    #[serde(rename = "logIndex")]
    pub log_index: String,
}

pub async fn run(pool: PgPool, config: Config) {
    let client = Client::builder()
        .timeout(Duration::from_secs(config.chain_rpc_timeout_secs))
        .build()
        .expect("chain RPC client");

    loop {
        if let Err(e) = scan_once(&pool, &config, &client).await {
            tracing::warn!("chain event scan failed: {e}");
        }
        tokio::time::sleep(Duration::from_secs(config.chain_poll_interval_secs)).await;
    }
}

async fn scan_once(pool: &PgPool, config: &Config, client: &Client) -> Result<()> {
    let mut lock_conn = pool.acquire().await?;
    let locked = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)")
        .bind(CHAIN_EVENTS_LOCK_KEY)
        .fetch_one(&mut *lock_conn)
        .await?;
    if !locked {
        return Ok(());
    }

    let result = async {
        let active_chains = active_chains(pool, config).await?;
        for chain in active_chains {
            if let Err(e) = scan_chain(pool, config, client, &chain).await {
                tracing::warn!(
                    chain_key = chain.spec.key,
                    chain_id = chain.spec.chain_id,
                    "chain event chain scan skipped: {e}"
                );
            }
        }
        Ok(())
    }
    .await;

    if let Err(e) = sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1)")
        .bind(CHAIN_EVENTS_LOCK_KEY)
        .fetch_one(&mut *lock_conn)
        .await
    {
        tracing::warn!("failed to release chain event advisory lock: {e}");
    }

    result
}

async fn active_chains(pool: &PgPool, config: &Config) -> Result<Vec<ActiveChain>> {
    let rows =
        sqlx::query("SELECT config FROM triggers WHERE kind = 'chain_event' AND enabled = true")
            .fetch_all(pool)
            .await?;

    let mut chains: BTreeMap<u64, ActiveChain> = BTreeMap::new();
    for row in rows {
        let cfg: Value = row.get("config");
        let parsed = match parse_chain_event_config(&cfg) {
            Ok(parsed) => parsed,
            Err(e) => {
                tracing::warn!("enabled chain_event trigger has invalid config; skipping: {e}");
                continue;
            }
        };
        let Some(spec) = chain_spec_by_id(parsed.chain_id) else {
            continue;
        };
        let Some(rpc_url) = std::env::var(spec.default_rpc_envvar)
            .ok()
            .filter(|v| !v.trim().is_empty())
        else {
            tracing::warn!(
                chain_key = spec.key,
                chain_id = spec.chain_id,
                rpc_envvar = spec.default_rpc_envvar,
                "chain RPC env var missing; skipping active chain_event triggers for chain"
            );
            continue;
        };
        chains
            .entry(spec.chain_id)
            .or_insert(ActiveChain { spec, rpc_url });
    }

    let _ = config; // keep signature symmetric with other worker helpers and future config resolution.
    Ok(chains.into_values().collect())
}

async fn scan_chain(
    pool: &PgPool,
    config: &Config,
    client: &Client,
    chain: &ActiveChain,
) -> Result<()> {
    let head = eth_block_number(client, &chain.rpc_url).await?;
    if head < config.chain_confirmation_depth {
        return Ok(());
    }
    let confirmed_head = head - config.chain_confirmation_depth;

    let rows = sqlx::query(
        "SELECT t.id, t.config, t.max_queued_runs, w.last_block
         FROM triggers t
         LEFT JOIN chain_watermarks w ON w.trigger_id = t.id
         WHERE t.kind = 'chain_event' AND t.enabled = true
         ORDER BY t.created_at ASC",
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        let trigger = ChainEventTrigger {
            id: row.get("id"),
            config: row.get("config"),
            max_queued_runs: row.get("max_queued_runs"),
            last_block: row.get("last_block"),
        };
        let parsed = match parse_chain_event_config(&trigger.config) {
            Ok(parsed) if parsed.chain_id == chain.spec.chain_id => parsed,
            Ok(_) => continue,
            Err(e) => {
                tracing::warn!(trigger_id = %trigger.id, "invalid chain_event config skipped: {e}");
                continue;
            }
        };
        if let Err(e) = scan_trigger(
            pool,
            config,
            client,
            chain,
            &trigger,
            &parsed,
            confirmed_head,
        )
        .await
        {
            tracing::warn!(trigger_id = %trigger.id, chain_key = chain.spec.key, "chain_event trigger scan skipped: {e}");
        }
    }
    Ok(())
}

async fn scan_trigger(
    pool: &PgPool,
    config: &Config,
    client: &Client,
    chain: &ActiveChain,
    trigger: &ChainEventTrigger,
    parsed: &ParsedChainEventConfig,
    confirmed_head: u64,
) -> Result<()> {
    let start_from = match trigger.last_block {
        Some(last) => (last.max(0) as u64).saturating_add(1),
        None => parsed
            .start_block
            .unwrap_or_else(|| confirmed_head.saturating_sub(config.chain_initial_lookback_blocks)),
    };
    if start_from > confirmed_head {
        return Ok(());
    }
    let to_block = confirmed_head
        .min(start_from.saturating_add(config.chain_max_block_range.saturating_sub(1)));

    let logs = eth_get_logs(client, &chain.rpc_url, parsed, start_from, to_block).await?;

    let mut tx = pool.begin().await?;
    let mut queue_saturated = false;
    for log in logs {
        let block_number = parse_hex_u64(&log.block_number).context("log blockNumber")?;
        let log_index = parse_hex_u64(&log.log_index).context("log logIndex")?;

        if queue_depth(&mut tx, trigger.id).await? >= max_queued_runs(trigger, config) {
            tracing::warn!(trigger_id = %trigger.id, "chain_event trigger queue is full; committing queued prefix and leaving watermark unchanged");
            queue_saturated = true;
            break;
        }

        let delivery_key = delivery_key(parsed.chain_id, &log.transaction_hash, log_index);
        let inserted_delivery = sqlx::query_scalar::<_, bool>(
            "INSERT INTO chain_event_deliveries (trigger_id, chain_id, tx_hash, log_index, delivery_key)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT DO NOTHING
             RETURNING true",
        )
        .bind(trigger.id)
        .bind(parsed.chain_id as i64)
        .bind(normalize_hex(&log.transaction_hash))
        .bind(log_index as i64)
        .bind(delivery_key)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(false);

        if !inserted_delivery {
            continue;
        }

        let input = build_chain_event_input(parsed, &log, block_number, log_index)?;
        let run_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO trigger_runs (id, trigger_id, status, input, attempt)
             VALUES ($1, $2, 'queued', $3, 1)",
        )
        .bind(run_id)
        .bind(trigger.id)
        .bind(input)
        .execute(&mut *tx)
        .await?;
        tracing::info!(trigger_id = %trigger.id, run_id = %run_id, block_number, "queued chain_event trigger run");
    }

    if !queue_saturated {
        sqlx::query(
            "INSERT INTO chain_watermarks (trigger_id, last_block, updated_at)
             VALUES ($1, $2, now())
             ON CONFLICT (trigger_id) DO UPDATE
             SET last_block = EXCLUDED.last_block, updated_at = now()",
        )
        .bind(trigger.id)
        .bind(to_block as i64)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn queue_depth(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    trigger_id: Uuid,
) -> Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM trigger_runs
         WHERE trigger_id = $1 AND status IN ('queued','running','retrying')",
    )
    .bind(trigger_id)
    .fetch_one(&mut **tx)
    .await
    .context("checking chain_event trigger queue depth")
}

fn max_queued_runs(trigger: &ChainEventTrigger, config: &Config) -> i64 {
    trigger
        .max_queued_runs
        .unwrap_or(config.webhook_default_max_queued_runs as i32)
        .max(0) as i64
}

async fn eth_block_number(client: &Client, rpc_url: &str) -> Result<u64> {
    let resp: RpcResponse<String> = client
        .post(rpc_url)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    rpc_result(resp).and_then(|hex| parse_hex_u64(&hex))
}

async fn eth_get_logs(
    client: &Client,
    rpc_url: &str,
    parsed: &ParsedChainEventConfig,
    from_block: u64,
    to_block: u64,
) -> Result<Vec<EvmLog>> {
    let mut topics = vec![Value::String(parsed.topic0.clone())];
    topics.extend(parsed.topic_filters.iter().cloned());
    let filter = json!({
        "fromBlock": u64_to_hex(from_block),
        "toBlock": u64_to_hex(to_block),
        "address": parsed.contract_address,
        "topics": topics,
    });
    let resp: RpcResponse<Vec<EvmLog>> = client
        .post(rpc_url)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"eth_getLogs","params":[filter]}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    rpc_result(resp)
}

fn rpc_result<T>(resp: RpcResponse<T>) -> Result<T> {
    match (resp.result, resp.error) {
        (Some(result), _) => Ok(result),
        (_, Some(err)) => anyhow::bail!("rpc error {}: {}", err.code, err.message),
        _ => anyhow::bail!("rpc response missing result"),
    }
}

pub fn parse_chain_event_config(config: &Value) -> Result<ParsedChainEventConfig> {
    let obj = config
        .as_object()
        .context("chain_event config must be object")?;
    let spec = if let Some(key) = obj.get("chain").and_then(Value::as_str) {
        chain_spec_by_key(key.trim()).context("unknown chain")?
    } else if let Some(id) = obj.get("chain_id").and_then(Value::as_u64) {
        chain_spec_by_id(id).context("unknown chain_id")?
    } else {
        anyhow::bail!("chain or chain_id required");
    };
    if let Some(id) = obj.get("chain_id").and_then(Value::as_u64) {
        if id != spec.chain_id {
            anyhow::bail!("chain and chain_id disagree");
        }
    }

    let contract_address = obj
        .get("contract_address")
        .or_else(|| obj.get("address"))
        .and_then(Value::as_str)
        .map(normalize_hex)
        .context("contract_address required")?;
    validate_hex_len(&contract_address, 20).context("invalid contract_address")?;

    let event_signature = obj
        .get("event_signature")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .context("event_signature required")?
        .to_string();
    validate_event_signature(&event_signature)?;
    let topic0 = event_topic0(&event_signature);

    let topic_filters = match obj.get("topic_filters") {
        Some(Value::Array(filters)) => {
            if filters.len() > 3 {
                anyhow::bail!("topic_filters can include at most 3 entries after topic0");
            }
            filters
                .iter()
                .map(normalize_topic_filter)
                .collect::<Result<Vec<_>>>()?
        }
        Some(_) => anyhow::bail!("topic_filters must be an array"),
        None => Vec::new(),
    };

    let start_block = match obj.get("start_block") {
        Some(Value::Number(n)) => Some(n.as_u64().context("start_block must be u64")?),
        Some(Value::String(s)) => Some(parse_hex_or_dec_u64(s).context("invalid start_block")?),
        Some(_) => anyhow::bail!("start_block must be integer or hex string"),
        None => None,
    };

    Ok(ParsedChainEventConfig {
        chain_key: spec.key,
        chain_id: spec.chain_id,
        contract_address,
        event_signature,
        topic0,
        topic_filters,
        start_block,
    })
}

pub fn event_topic0(event_signature: &str) -> String {
    format!("0x{}", hex::encode(keccak256(event_signature.as_bytes())))
}

fn validate_event_signature(sig: &str) -> Result<()> {
    let open = sig.find('(').context("event signature missing '('")?;
    let close = sig.rfind(')').context("event signature missing ')'")?;
    if open == 0 || close != sig.len() - 1 || close < open {
        anyhow::bail!("invalid event signature");
    }
    for ty in event_param_types(sig)? {
        validate_abi_type(&ty)?;
    }
    Ok(())
}

fn validate_abi_type(ty: &str) -> Result<()> {
    if matches!(ty, "address" | "bool" | "string" | "bytes" | "bytes32") {
        return Ok(());
    }
    if ty.starts_with("uint") || ty.starts_with("int") {
        let bits = &ty[ty.find('t').unwrap() + 1..];
        if bits.is_empty()
            || (bits
                .parse::<u16>()
                .is_ok_and(|b| b > 0 && b <= 256 && b % 8 == 0))
        {
            return Ok(());
        }
    }
    anyhow::bail!("unsupported abi type: {ty}")
}

pub fn validate_topic_filter(value: &Value) -> Result<()> {
    normalize_topic_filter(value).map(|_| ())
}

fn normalize_topic_filter(value: &Value) -> Result<Value> {
    match value {
        Value::Null => Ok(Value::Null),
        Value::String(s) => normalize_topic(s).map(Value::String),
        Value::Array(items) => {
            if items.is_empty() {
                anyhow::bail!("topic filter alternatives cannot be empty");
            }
            let mut normalized = Vec::with_capacity(items.len());
            for item in items {
                let s = item
                    .as_str()
                    .context("topic filter alternatives must be strings")?;
                normalized.push(Value::String(normalize_topic(s)?));
            }
            Ok(Value::Array(normalized))
        }
        _ => anyhow::bail!("topic filter must be null, string, or string array"),
    }
}

fn normalize_topic(topic: &str) -> Result<String> {
    let normalized = normalize_hex(topic);
    validate_hex_len(&normalized, 32)?;
    Ok(normalized)
}

fn validate_hex_len(value: &str, bytes: usize) -> Result<()> {
    let hex = value
        .strip_prefix("0x")
        .context("hex value must be 0x-prefixed")?;
    if hex.len() != bytes * 2 {
        anyhow::bail!("hex value must be {bytes} bytes");
    }
    hex::decode(hex).context("invalid hex")?;
    Ok(())
}

fn normalize_hex(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(rest) = trimmed.strip_prefix("0X") {
        format!("0x{}", rest.to_ascii_lowercase())
    } else if let Some(rest) = trimmed.strip_prefix("0x") {
        format!("0x{}", rest.to_ascii_lowercase())
    } else {
        format!("0x{}", trimmed.to_ascii_lowercase())
    }
}

pub fn parse_hex_u64(value: &str) -> Result<u64> {
    let hex = value
        .strip_prefix("0x")
        .context("hex quantity must be 0x-prefixed")?;
    u64::from_str_radix(hex, 16).context("invalid hex quantity")
}

fn parse_hex_or_dec_u64(value: &str) -> Result<u64> {
    if value.trim_start().starts_with("0x") {
        parse_hex_u64(value.trim())
    } else {
        value
            .trim()
            .parse::<u64>()
            .context("invalid decimal quantity")
    }
}

pub fn u64_to_hex(value: u64) -> String {
    format!("0x{value:x}")
}

pub fn delivery_key(chain_id: u64, tx_hash: &str, log_index: u64) -> String {
    format!("{chain_id}:{}:{log_index}", normalize_hex(tx_hash))
}

pub fn build_chain_event_input(
    parsed: &ParsedChainEventConfig,
    log: &EvmLog,
    block_number: u64,
    log_index: u64,
) -> Result<Value> {
    Ok(json!({
        "source": "chain_event",
        "event": {
            "source": "chain_event",
            "chain_key": parsed.chain_key,
            "chain_id": parsed.chain_id,
            "contract_address": parsed.contract_address,
            "event_signature": parsed.event_signature,
            "topic0": parsed.topic0,
            "block_number": block_number,
            "transaction_hash": normalize_hex(&log.transaction_hash),
            "log_index": log_index,
            "address": normalize_hex(&log.address),
            "topics": log.topics.iter().map(|t| normalize_hex(t)).collect::<Vec<_>>(),
            "data": normalize_hex(&log.data),
            "decoded": decode_event_best_effort(&parsed.event_signature, log),
            "raw_log": log,
        }
    }))
}

pub fn decode_event_best_effort(event_signature: &str, log: &EvmLog) -> Value {
    let Ok(types) = event_param_types(event_signature) else {
        return Value::Null;
    };
    let indexed_count = log.topics.len().saturating_sub(1).min(types.len());
    let mut params = serde_json::Map::new();
    for (i, ty) in types.iter().take(indexed_count).enumerate() {
        let value = decode_topic_value(ty, &log.topics[i + 1]);
        params.insert(format!("arg{i}"), value);
    }
    let data_words = decode_data_words(&log.data);
    for (offset, (i, ty)) in types.iter().enumerate().skip(indexed_count).enumerate() {
        let value = decode_data_value(ty, &data_words, offset);
        params.insert(format!("arg{i}"), value);
    }
    Value::Object(params)
}

fn event_param_types(sig: &str) -> Result<Vec<String>> {
    let open = sig.find('(').context("event signature missing '('")?;
    let close = sig.rfind(')').context("event signature missing ')'")?;
    let inner = &sig[open + 1..close];
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(inner.split(',').map(|s| s.trim().to_string()).collect())
}

fn decode_topic_value(ty: &str, topic: &str) -> Value {
    let topic = normalize_hex(topic);
    match ty {
        "address" => Value::String(format!("0x{}", &topic[topic.len() - 40..])),
        "bool" => Value::Bool(parse_hex_u64(&topic).unwrap_or(0) != 0),
        _ => Value::String(topic),
    }
}

fn decode_data_words(data: &str) -> Vec<String> {
    let data = normalize_hex(data);
    let Some(hex) = data.strip_prefix("0x") else {
        return Vec::new();
    };
    hex.as_bytes()
        .chunks(64)
        .filter(|chunk| chunk.len() == 64)
        .map(|chunk| format!("0x{}", std::str::from_utf8(chunk).unwrap_or_default()))
        .collect()
}

fn decode_data_value(ty: &str, words: &[String], index: usize) -> Value {
    let Some(word) = words.get(index) else {
        return Value::Null;
    };
    match ty {
        "address" => Value::String(format!("0x{}", &word[word.len() - 40..])),
        "bool" => Value::Bool(parse_hex_u64(word).unwrap_or(0) != 0),
        "string" | "bytes" => {
            decode_dynamic_value(ty, words, index).unwrap_or(Value::String(word.clone()))
        }
        _ if ty.starts_with("uint") => Value::String(
            parse_hex_u128(word)
                .map(|v| v.to_string())
                .unwrap_or_else(|| word.clone()),
        ),
        _ if ty.starts_with("int") || ty == "bytes32" => Value::String(word.clone()),
        _ => Value::String(word.clone()),
    }
}

fn parse_hex_u128(value: &str) -> Option<u128> {
    u128::from_str_radix(value.strip_prefix("0x")?, 16).ok()
}

fn decode_dynamic_value(ty: &str, words: &[String], index: usize) -> Option<Value> {
    let offset_bytes = parse_hex_u128(words.get(index)?)? as usize;
    let offset_words = offset_bytes / 32;
    let len = parse_hex_u128(words.get(offset_words)?)? as usize;
    let mut bytes = Vec::new();
    let mut remaining = len;
    let mut word_index = offset_words + 1;
    while remaining > 0 {
        let word_hex = words.get(word_index)?.strip_prefix("0x")?;
        let chunk = hex::decode(word_hex).ok()?;
        let take = remaining.min(32);
        bytes.extend_from_slice(&chunk[..take]);
        remaining -= take;
        word_index += 1;
    }
    if ty == "string" {
        String::from_utf8(bytes).ok().map(Value::String)
    } else {
        Some(Value::String(format!("0x{}", hex::encode(bytes))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn computes_transfer_topic0() {
        assert_eq!(
            event_topic0("Transfer(address,address,uint256)"),
            "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
        );
    }

    #[test]
    fn validates_chain_event_config_by_key() {
        let parsed = parse_chain_event_config(&json!({
            "chain": "base",
            "contract_address": "0x0000000000000000000000000000000000000001",
            "event_signature": "Transfer(address,address,uint256)",
            "topic_filters": [null, "0X0000000000000000000000000000000000000000000000000000000000000002"],
            "start_block": "0x10"
        })).unwrap();
        assert_eq!(parsed.chain_id, 8453);
        assert_eq!(parsed.start_block, Some(16));
        assert_eq!(
            parsed.topic_filters[1],
            json!("0x0000000000000000000000000000000000000000000000000000000000000002")
        );
    }

    #[test]
    fn rejects_invalid_topics_and_addresses() {
        assert!(parse_chain_event_config(&json!({
            "chain_id": 1,
            "contract_address": "0x1234",
            "event_signature": "Transfer(address,address,uint256)"
        }))
        .is_err());
        assert!(parse_chain_event_config(&json!({
            "chain_id": 1,
            "contract_address": "0x0000000000000000000000000000000000000001",
            "event_signature": "Transfer(address,address,uint256)",
            "topic_filters": [null, null, null, null]
        }))
        .is_err());
        assert!(validate_topic_filter(&json!(["0x1234"])).is_err());
    }

    #[test]
    fn parses_and_formats_block_hex() {
        assert_eq!(parse_hex_u64("0x10").unwrap(), 16);
        assert_eq!(u64_to_hex(500), "0x1f4");
    }

    #[test]
    fn delivery_key_is_deterministic_and_normalized() {
        assert_eq!(delivery_key(1, "0XABC", 7), delivery_key(1, "0xabc", 7));
    }

    #[test]
    fn builds_payload_under_params_event() {
        let parsed = parse_chain_event_config(&json!({
            "chain": "ethereum",
            "contract_address": "0x0000000000000000000000000000000000000001",
            "event_signature": "Transfer(address,address,uint256)"
        }))
        .unwrap();
        let log = EvmLog {
            address: "0x0000000000000000000000000000000000000001".into(),
            topics: vec![
                parsed.topic0.clone(),
                "0x0000000000000000000000000000000000000000000000000000000000000002".into(),
                "0x0000000000000000000000000000000000000000000000000000000000000003".into(),
            ],
            data: "0x000000000000000000000000000000000000000000000000000000000000007b".into(),
            block_number: "0x64".into(),
            transaction_hash: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            log_index: "0x0".into(),
        };
        let payload = build_chain_event_input(&parsed, &log, 100, 0).unwrap();
        assert_eq!(payload["source"], "chain_event");
        assert_eq!(payload["event"]["decoded"]["arg2"], "123");
    }

    #[test]
    fn decodes_dynamic_string_best_effort() {
        let log = EvmLog {
            address: "0x0000000000000000000000000000000000000001".into(),
            topics: vec![event_topic0("Message(string)")],
            data: concat!(
                "0x",
                "0000000000000000000000000000000000000000000000000000000000000020",
                "0000000000000000000000000000000000000000000000000000000000000002",
                "6869000000000000000000000000000000000000000000000000000000000000"
            )
            .into(),
            block_number: "0x1".into(),
            transaction_hash: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            log_index: "0x0".into(),
        };
        assert_eq!(
            decode_event_best_effort("Message(string)", &log)["arg0"],
            "hi"
        );
    }
}
