use crate::accounts::can_execute_action;
use crate::accounts::chain_config::{ChainConfig, ConfigKeys};
use crate::actions::client::ClientBuilder;
use crate::actions::client::models::DenoExecutionEnv;
use crate::actions::client::{
    MAX_ASYNC_TIMEOUT_MS, MAX_CLIENT_TIMEOUT_MS_BUFFER, MAX_MAX_CODE_LENGTH,
    MAX_MAX_CONSOLE_LOG_LENGTH, MAX_MAX_FETCH_COUNT, MAX_MAX_GET_KEYS_COUNT,
    MAX_MAX_RESPONSE_LENGTH, MAX_MAX_RETRIES, MAX_MEMORY_LIMIT_MB, MAX_TIMEOUT_MS,
};
use crate::actions::grpc::GrpcClientPool;
use crate::core::v1::helpers::api_status::ApiStatus;
use crate::core::v1::models::request::LitActionRequest;
use crate::core::v1::models::response::{LitActionClientConfigResponse, LitActionResponse};
use crate::observability::RequestSpan;
use crate::stripe::StripeState;
use crate::utils::parse_with_hash::ipfs_cid_to_u256;
use ipfs_hasher::IpfsHasher;
use moka::future::Cache;
use rocket::serde::json::Json;
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tracing::{Instrument, instrument};

#[allow(clippy::too_many_arguments)]
#[instrument(name = "core_features::lit_action", level = "debug", skip_all, err)]
pub async fn lit_action(
    request_span: &RequestSpan,
    api_key: &str,
    grpc_client_pool: &GrpcClientPool<tonic::transport::Channel>,
    ipfs_cache: &Cache<String, Arc<String>>,
    http_client: &reqwest::Client,
    chain_config: Arc<ChainConfig>,
    stripe_state: Option<Arc<StripeState>>,
    lit_action_request: Json<LitActionRequest>,
) -> Result<LitActionResponse, ApiStatus> {
    let request_id = request_span.request_id.clone();

    let mut http_headers = BTreeMap::new();
    http_headers.insert("x-request-id".to_string(), request_id.clone());
    if let Some(ref cid) = request_span.correlation_id {
        http_headers.insert("x-correlation-id".to_string(), cid.clone());
    }

    let (code_to_run, derived_ipfs_id) = resolve_action_code(
        &lit_action_request.code,
        &lit_action_request.ipfs_id,
        ipfs_cache,
    )
    .await?;
    let cid_hash = ipfs_cid_to_u256(&derived_ipfs_id)?;
    let can_execute = can_execute_action(api_key, cid_hash)
        .instrument(tracing::debug_span!("lit_action::can_execute_action"))
        .await?;
    if !can_execute {
        let msg = format!(
            "The provided API key is not authorized to execute the specified action ({derived_ipfs_id}/{cid_hash})."
        );
        return Err(ApiStatus::forbidden(msg));
    }

    // Cache after authorization so unauthorized requests cannot pollute the cache.
    ipfs_cache
        .insert(derived_ipfs_id.clone(), Arc::new(code_to_run.clone()))
        .await;

    let deno_execution_env = DenoExecutionEnv {
        ipfs_cache: Some(moka::future::Cache::clone(ipfs_cache)),
        http_client: Some(reqwest::Client::clone(http_client)),
    };

    let mut builder = get_lit_action_client_builder(chain_config)
        .instrument(tracing::debug_span!("lit_action::build_client_config"))
        .await;
    builder
        .js_env(deno_execution_env)
        .request_id(request_id.clone())
        .http_headers(http_headers)
        .api_key(api_key.to_string())
        .ipfs_id(derived_ipfs_id.clone())
        .client_grpc_channels((*grpc_client_pool).clone());

    if let Some(stripe) = stripe_state {
        builder.stripe_state(stripe);
    }

    let mut client = match builder.build().map_err(|e| e.to_string()) {
        Ok(client) => client,
        Err(e) => return Err(anyhow::anyhow!("failed to build client: {:?}", e).into()),
    };

    let js_params = lit_action_request.js_params.clone();
    let execution_options = crate::actions::client::models::ExecutionOptions {
        code: code_to_run,
        globals: js_params.clone(),
        action_ipfs_id: Some(derived_ipfs_id),
    };

    let result = match client
        .execute_js(execution_options)
        .instrument(tracing::debug_span!("lit_action::execute_js"))
        .await
    {
        Ok(result) => result,
        Err(e) => return Err(anyhow::anyhow!("Actions failed with : {:?}", e).into()),
    };

    let response = match serde_json::from_str::<serde_json::Value>(&result.response) {
        Ok(response) => response,
        Err(e) => {
            tracing::error!("failed to parse response: {:?}", e);
            json!(result.response)
        }
    };

    let lit_action_response = LitActionResponse {
        response,
        logs: result.logs,
        has_error: false,
    };

    Ok(lit_action_response)
}

pub async fn get_lit_action_client_config(
    chain_config: Arc<ChainConfig>,
) -> Result<LitActionClientConfigResponse, ApiStatus> {
    let builder = get_lit_action_client_builder(chain_config).await;
    let client = builder
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build client: {e}"))?;
    Ok(client.config_snapshot())
}

fn get_lit_action_ipfs_id(code: &str) -> String {
    let ipfs_hasher = IpfsHasher::default();
    ipfs_hasher.compute(code.as_bytes())
}

/// Resolve the action code and its IPFS ID without modifying the cache.
///
/// * If `code` is provided, compute the IPFS hash and return both.
/// * If only `ipfs_id` is provided, attempt to retrieve the code from the cache.
/// * If neither is provided, return an error.
///
/// Callers should insert into the cache **after** authorization succeeds to prevent
/// unauthorized requests from polluting the shared cache.
async fn resolve_action_code(
    code: &Option<String>,
    ipfs_id: &Option<String>,
    ipfs_cache: &Cache<String, Arc<String>>,
) -> Result<(String, String), ApiStatus> {
    let code_str = code.as_deref().filter(|s| !s.is_empty());
    let ipfs_id_str = ipfs_id.as_deref().filter(|s| !s.is_empty());

    match (code_str, ipfs_id_str) {
        (Some(code), ipfs_id_hint) => {
            let derived_ipfs_id = get_lit_action_ipfs_id(code);
            if let Some(hint) = ipfs_id_hint
                && hint != derived_ipfs_id
            {
                tracing::debug!(
                    supplied_ipfs_id = hint,
                    derived_ipfs_id = %derived_ipfs_id,
                    "Supplied ipfs_id does not match derived hash of code; using derived hash"
                );
            }
            Ok((code.to_string(), derived_ipfs_id))
        }
        (None, Some(ipfs_id)) => {
            if let Some(cached_code) = ipfs_cache.get(ipfs_id).await {
                Ok(((*cached_code).clone(), ipfs_id.to_string()))
            } else {
                Err(ApiStatus::bad_request(
                    anyhow::anyhow!("cache miss for IPFS ID {ipfs_id}"),
                    "No cached code found. Submit the action code at least once before referencing it by IPFS ID.",
                ))
            }
        }
        (None, None) => Err(ApiStatus::bad_request(
            anyhow::anyhow!("missing code and ipfs_id"),
            "Either `code` or `ipfs_id` must be provided.",
        )),
    }
}

/// Parse a value from the chain config snapshot, validating it's within bounds.
/// Returns `Some(value)` if valid, `None` otherwise (falls back to builder default).
fn parse_config_value<T>(
    snapshot: &HashMap<String, String>,
    key: &ConfigKeys,
    min: T,
    max: T,
) -> Option<T>
where
    T: std::str::FromStr + PartialOrd + std::fmt::Display,
{
    let val_str = snapshot.get(&key.to_string())?;
    let val = val_str.parse::<T>().ok()?;
    if val > min && val <= max {
        Some(val)
    } else {
        tracing::warn!(
            "chain_config: {key} value {val} out of bounds (>{min}, <={max}), using default"
        );
        None
    }
}

async fn get_lit_action_client_builder(chain_config: Arc<ChainConfig>) -> ClientBuilder {
    let mut builder = ClientBuilder::default();

    let keys = vec![
        ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS,
        ConfigKeys::LIT_ACTION_DEFAULT_ASYNC_TIMEOUT_MS,
        ConfigKeys::LIT_ACTION_DEFAULT_MEMORY_LIMIT_MB,
        ConfigKeys::LIT_ACTION_DEFAULT_MAX_CODE_LENGTH,
        ConfigKeys::LIT_ACTION_DEFAULT_MAX_CONSOLE_LOG_LENGTH,
        ConfigKeys::LIT_ACTION_DEFAULT_MAX_FETCH_COUNT,
        ConfigKeys::LIT_ACTION_DEFAULT_MAX_RESPONSE_LENGTH,
        ConfigKeys::LIT_ACTION_DEFAULT_MAX_GET_KEYS_COUNT,
        ConfigKeys::LIT_ACTION_DEFAULT_MAX_RETRIES,
        ConfigKeys::LIT_ACTION_DEFAULT_CLIENT_TIMEOUT_MS_BUFFER,
    ];

    let snapshot = match chain_config.get_many(keys).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("chain_config: get_many failed: {e}, using all defaults");
            return builder;
        }
    };

    if let Some(v) = parse_config_value::<u64>(
        &snapshot,
        &ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS,
        0,
        MAX_TIMEOUT_MS,
    ) {
        builder.timeout_ms(v);
    }
    if let Some(v) = parse_config_value::<u64>(
        &snapshot,
        &ConfigKeys::LIT_ACTION_DEFAULT_ASYNC_TIMEOUT_MS,
        0,
        MAX_ASYNC_TIMEOUT_MS,
    ) {
        builder.async_timeout_ms(v);
    }
    if let Some(v) = parse_config_value::<u32>(
        &snapshot,
        &ConfigKeys::LIT_ACTION_DEFAULT_MEMORY_LIMIT_MB,
        0,
        MAX_MEMORY_LIMIT_MB,
    ) {
        builder.memory_limit_mb(v);
    }
    if let Some(v) = parse_config_value::<u64>(
        &snapshot,
        &ConfigKeys::LIT_ACTION_DEFAULT_MAX_CODE_LENGTH,
        0,
        MAX_MAX_CODE_LENGTH,
    ) {
        builder.max_code_length(v);
    }
    if let Some(v) = parse_config_value::<u64>(
        &snapshot,
        &ConfigKeys::LIT_ACTION_DEFAULT_MAX_CONSOLE_LOG_LENGTH,
        0,
        MAX_MAX_CONSOLE_LOG_LENGTH,
    ) {
        builder.max_console_log_length(v);
    }
    if let Some(v) = parse_config_value::<u32>(
        &snapshot,
        &ConfigKeys::LIT_ACTION_DEFAULT_MAX_FETCH_COUNT,
        0,
        MAX_MAX_FETCH_COUNT,
    ) {
        builder.max_fetch_count(v);
    }
    if let Some(v) = parse_config_value::<u64>(
        &snapshot,
        &ConfigKeys::LIT_ACTION_DEFAULT_MAX_RESPONSE_LENGTH,
        0,
        MAX_MAX_RESPONSE_LENGTH,
    ) {
        builder.max_response_length(v);
    }
    if let Some(v) = parse_config_value::<u32>(
        &snapshot,
        &ConfigKeys::LIT_ACTION_DEFAULT_MAX_GET_KEYS_COUNT,
        0,
        MAX_MAX_GET_KEYS_COUNT,
    ) {
        builder.max_get_keys_count(v);
    }
    if let Some(v) = parse_config_value::<u32>(
        &snapshot,
        &ConfigKeys::LIT_ACTION_DEFAULT_MAX_RETRIES,
        0,
        MAX_MAX_RETRIES,
    ) {
        builder.max_retries(v);
    }
    if let Some(v) = parse_config_value::<u64>(
        &snapshot,
        &ConfigKeys::LIT_ACTION_DEFAULT_CLIENT_TIMEOUT_MS_BUFFER,
        0,
        MAX_CLIENT_TIMEOUT_MS_BUFFER,
    ) {
        builder.client_timeout_ms_buffer(v);
    }

    builder
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_value_valid() {
        let mut snapshot = HashMap::new();
        snapshot.insert(
            ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS.to_string(),
            "60000".to_string(),
        );
        let result = parse_config_value::<u64>(
            &snapshot,
            &ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS,
            0,
            MAX_TIMEOUT_MS,
        );
        assert_eq!(result, Some(60000));
    }

    #[test]
    fn parse_config_value_zero_rejected() {
        let mut snapshot = HashMap::new();
        snapshot.insert(
            ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS.to_string(),
            "0".to_string(),
        );
        let result = parse_config_value::<u64>(
            &snapshot,
            &ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS,
            0,
            MAX_TIMEOUT_MS,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn parse_config_value_exceeds_max() {
        let mut snapshot = HashMap::new();
        snapshot.insert(
            ConfigKeys::LIT_ACTION_DEFAULT_MEMORY_LIMIT_MB.to_string(),
            "99999".to_string(),
        );
        let result = parse_config_value::<u32>(
            &snapshot,
            &ConfigKeys::LIT_ACTION_DEFAULT_MEMORY_LIMIT_MB,
            0,
            MAX_MEMORY_LIMIT_MB,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn parse_config_value_unparseable() {
        let mut snapshot = HashMap::new();
        snapshot.insert(
            ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS.to_string(),
            "not_a_number".to_string(),
        );
        let result = parse_config_value::<u64>(
            &snapshot,
            &ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS,
            0,
            MAX_TIMEOUT_MS,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn parse_config_value_missing_key() {
        let snapshot = HashMap::new();
        let result = parse_config_value::<u64>(
            &snapshot,
            &ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS,
            0,
            MAX_TIMEOUT_MS,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn parse_config_value_at_max_boundary() {
        let mut snapshot = HashMap::new();
        snapshot.insert(
            ConfigKeys::LIT_ACTION_DEFAULT_MEMORY_LIMIT_MB.to_string(),
            MAX_MEMORY_LIMIT_MB.to_string(),
        );
        let result = parse_config_value::<u32>(
            &snapshot,
            &ConfigKeys::LIT_ACTION_DEFAULT_MEMORY_LIMIT_MB,
            0,
            MAX_MEMORY_LIMIT_MB,
        );
        assert_eq!(result, Some(MAX_MEMORY_LIMIT_MB));
    }

    #[test]
    fn config_snapshot_reflects_actual_values() {
        use crate::actions::client::{DEFAULT_ASYNC_TIMEOUT_MS, DEFAULT_CLIENT_TIMEOUT_MS_BUFFER};

        let client = ClientBuilder::default()
            .async_timeout_ms(42_000u64)
            .client_timeout_ms_buffer(10_000u64)
            .build()
            .unwrap();

        let snapshot = client.config_snapshot();
        assert_eq!(snapshot.async_timeout_ms, 42_000);
        assert_ne!(snapshot.async_timeout_ms, DEFAULT_ASYNC_TIMEOUT_MS);
        assert_eq!(snapshot.client_timeout_ms_buffer, 10_000);
        assert_ne!(
            snapshot.client_timeout_ms_buffer,
            DEFAULT_CLIENT_TIMEOUT_MS_BUFFER
        );
    }

    #[test]
    fn client_timeout_uses_instance_buffer() {
        use crate::actions::client::DEFAULT_CLIENT_TIMEOUT_MS_BUFFER;

        let client = ClientBuilder::default()
            .timeout_ms(1_000u64)
            .client_timeout_ms_buffer(500u64)
            .build()
            .unwrap();

        assert_eq!(
            client.client_timeout(),
            tokio::time::Duration::from_millis(1_500)
        );
        // Verify it's NOT using the default constant
        assert_ne!(500u64, DEFAULT_CLIENT_TIMEOUT_MS_BUFFER);
    }

    fn test_cache() -> Cache<String, Arc<String>> {
        Cache::new(100)
    }

    #[tokio::test]
    async fn resolve_action_code_returns_inline_code_without_caching() {
        let cache = test_cache();
        let code = Some("console.log('hello')".to_string());
        let (returned_code, ipfs_id) = resolve_action_code(&code, &None, &cache).await.unwrap();
        assert_eq!(returned_code, "console.log('hello')");
        assert_eq!(ipfs_id, get_lit_action_ipfs_id(&returned_code));
        // resolve_action_code does NOT insert into cache; caller caches after auth.
        assert!(cache.get(&ipfs_id).await.is_none());
    }

    #[tokio::test]
    async fn resolve_action_code_retrieves_from_cache() {
        let cache = test_cache();
        let code = "console.log('cached')".to_string();
        let ipfs_id = get_lit_action_ipfs_id(&code);
        cache.insert(ipfs_id.clone(), Arc::new(code.clone())).await;

        let (returned_code, returned_id) =
            resolve_action_code(&None, &Some(ipfs_id.clone()), &cache)
                .await
                .unwrap();
        assert_eq!(returned_code, code);
        assert_eq!(returned_id, ipfs_id);
    }

    #[tokio::test]
    async fn resolve_action_code_cache_miss_errors() {
        let cache = test_cache();
        let result = resolve_action_code(&None, &Some("QmUnknown".to_string()), &cache).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn resolve_action_code_neither_provided_errors() {
        let cache = test_cache();
        let result = resolve_action_code(&None, &None, &cache).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn resolve_action_code_prefers_inline_code() {
        let cache = test_cache();
        // Pre-populate cache with different code under the same ID
        let code = "console.log('new')".to_string();
        let ipfs_id = get_lit_action_ipfs_id(&code);
        cache
            .insert(ipfs_id.clone(), Arc::new("console.log('old')".to_string()))
            .await;

        // When both code and ipfs_id are provided, code wins
        let (returned_code, _) = resolve_action_code(&Some(code.clone()), &Some(ipfs_id), &cache)
            .await
            .unwrap();
        assert_eq!(returned_code, code);
    }

    #[tokio::test]
    async fn resolve_action_code_empty_strings_treated_as_none() {
        let cache = test_cache();
        let result =
            resolve_action_code(&Some("".to_string()), &Some("".to_string()), &cache).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn resolve_action_code_mismatched_ipfs_id_uses_derived() {
        let cache = test_cache();
        let old_code = "console.log('old')";
        let old_ipfs_id = get_lit_action_ipfs_id(old_code);
        cache
            .insert(old_ipfs_id.clone(), Arc::new(old_code.to_string()))
            .await;

        let new_code = "console.log('new')".to_string();
        // Caller provides new code but references the OLD ipfs_id
        let (returned_code, returned_id) =
            resolve_action_code(&Some(new_code.clone()), &Some(old_ipfs_id.clone()), &cache)
                .await
                .unwrap();

        assert_eq!(returned_code, new_code);
        assert_ne!(returned_id, old_ipfs_id);
        // Old cache entry must remain untouched
        assert_eq!(&*cache.get(&old_ipfs_id).await.unwrap(), old_code);
        // resolve_action_code does not cache the new code; caller does after auth
        assert!(cache.get(&returned_id).await.is_none());
    }

    #[test]
    fn serde_backward_compat_missing_buffer() {
        use crate::actions::client::DEFAULT_CLIENT_TIMEOUT_MS_BUFFER;

        let client = ClientBuilder::default().build().unwrap();
        let mut json_val = serde_json::to_value(&client).unwrap();
        // Remove the field to simulate an old serialized ActionJob
        json_val
            .as_object_mut()
            .unwrap()
            .remove("client_timeout_ms_buffer");
        let deserialized: crate::actions::client::Client =
            serde_json::from_value(json_val).unwrap();
        assert_eq!(
            deserialized.config_snapshot().client_timeout_ms_buffer,
            DEFAULT_CLIENT_TIMEOUT_MS_BUFFER
        );
    }
}
