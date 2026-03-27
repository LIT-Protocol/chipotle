use crate::accounts::can_execute_action;
use crate::accounts::chain_config::{ChainConfig, ConfigKeys};
use crate::actions::client::ClientBuilder;
use crate::actions::client::models::DenoExecutionEnv;
use crate::actions::grpc::GrpcClientPool;
use crate::core::v1::helpers::api_status::ApiStatus;
use crate::core::v1::models::request::LitActionRequest;
use crate::core::v1::models::response::{LitActionClientConfigResponse, LitActionResponse};
use crate::observability::RequestSpan;
use crate::utils::parse_with_hash::ipfs_cid_to_u256;
use ipfs_hasher::IpfsHasher;
use moka::future::Cache;
use rocket::serde::json::Json;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tracing::instrument;

/// Cache for IPFS hash computation. Keyed by a fast hash of the code string.
static IPFS_HASH_CACHE: std::sync::LazyLock<moka::sync::Cache<u64, String>> =
    std::sync::LazyLock::new(|| moka::sync::Cache::builder().max_capacity(10_000).build());

#[instrument(name = "core_features::lit_action", level = "debug", skip_all, err)]
pub async fn lit_action(
    request_span: &RequestSpan,
    api_key: &str,
    grpc_client_pool: &GrpcClientPool<tonic::transport::Channel>,
    ipfs_cache: &Cache<String, String>,
    http_client: &reqwest::Client,
    chain_config: Arc<ChainConfig>,
    lit_action_request: Json<LitActionRequest>,
) -> Result<LitActionResponse, ApiStatus> {
    let request_id = request_span.request_id.clone();

    let mut http_headers = BTreeMap::new();
    http_headers.insert("x-request-id".to_string(), request_id.clone());
    if let Some(ref cid) = request_span.correlation_id {
        http_headers.insert("x-correlation-id".to_string(), cid.clone());
    }

    let code_to_run = lit_action_request.code.clone();
    let derived_ipfs_id = get_lit_action_ipfs_id(code_to_run.clone());
    let cid_hash = ipfs_cid_to_u256(&derived_ipfs_id)?;

    // Run auth check and client builder concurrently — they are independent
    let (auth_result, builder) = tokio::join!(
        can_execute_action(api_key, cid_hash),
        get_lit_action_client_builder(chain_config)
    );
    if !auth_result? {
        let msg = format!(
            "The provided API key is not authorized to execute the specified action ({derived_ipfs_id}/{cid_hash})."
        );
        return Err(ApiStatus::forbidden(msg));
    }

    let deno_execution_env = DenoExecutionEnv {
        ipfs_cache: Some(moka::future::Cache::clone(ipfs_cache)),
        http_client: Some(reqwest::Client::clone(http_client)),
    };

    let mut builder = builder;
    builder
        .js_env(deno_execution_env)
        .request_id(request_id.clone())
        .http_headers(http_headers)
        .api_key(api_key.to_string())
        .ipfs_id(derived_ipfs_id.clone())
        .client_grpc_channels((*grpc_client_pool).clone());

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

    let result = match client.execute_js(execution_options).await {
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

fn get_lit_action_ipfs_id(code: String) -> String {
    let mut hasher = DefaultHasher::new();
    code.hash(&mut hasher);
    let code_hash = hasher.finish();

    IPFS_HASH_CACHE.get_with(code_hash, || {
        let ipfs_hasher = IpfsHasher::default();
        ipfs_hasher.compute(code.as_bytes())
    })
}

async fn get_lit_action_client_builder(chain_config: Arc<ChainConfig>) -> ClientBuilder {
    let mut builder = ClientBuilder::default();
    if let Ok(Some(val)) = chain_config
        .get(ConfigKeys::LIT_ACTION_DEFAULT_TIMEOUT_MS)
        .await
        && let Ok(ms) = val.parse::<u64>()
    {
        builder.timeout_ms(ms);
    }

    if let Ok(Some(val)) = chain_config
        .get(ConfigKeys::LIT_ACTION_DEFAULT_MEMORY_LIMIT_MB)
        .await
        && let Ok(mb) = val.parse::<u32>()
    {
        builder.memory_limit_mb(mb);
    }

    builder
}
