use crate::actions::client::ClientBuilder;
use crate::actions::client::models::DenoExecutionEnv;
use crate::actions::grpc::GrpcClientPool;
use crate::core::api_status::ApiStatus;
use crate::core::v1::models::request::{LitActionRequest, SignWithPKPRequest};
use crate::core::v1::models::response::{
    LitActionResponse, LitActionSignature, SignWithPkpResponse,
};
use ipfs_hasher::IpfsHasher;
use moka::future::Cache;
use rocket::serde::json::Json;
use tracing::instrument;

fn not_configured() -> ApiStatus {
    ApiStatus::internal_server_error(
        anyhow::anyhow!("Lit testnet not configured"),
        "This operation is not available.",
    )
}

pub async fn sign_with_pkp(
    _sign_request: Json<SignWithPKPRequest>,
) -> Result<SignWithPkpResponse, ApiStatus> {
    Err(not_configured())
}

// #[instrument(level = "debug", skip(api_key, grpc_client_pool, ipfs_cache, http_client), err)]
pub async fn lit_action(
    api_key: &str,
    grpc_client_pool: &GrpcClientPool<tonic::transport::Channel>,
    ipfs_cache: &Cache<String, String>,
    http_client: &reqwest::Client,
    lit_action_request: Json<LitActionRequest>,
) -> Result<LitActionResponse, ApiStatus> {
    let request_id = Some("test".to_string());

    let deno_execution_env = DenoExecutionEnv {
        ipfs_cache: Some(moka::future::Cache::clone(ipfs_cache)),
        http_client: Some(reqwest::Client::clone(http_client)),
    };

    let mut client = match ClientBuilder::default()
        .js_env(deno_execution_env)
        .request_id(request_id.clone())
        .api_key(api_key.to_string())
        .client_grpc_channels((*grpc_client_pool).clone())
        .build()
        .map_err(|e| e.to_string())
    {
        Ok(client) => client,
        Err(e) => return Err(anyhow::anyhow!("failed to build client: {:?}", e).into()),
    };

    let code_to_run = lit_action_request.code.clone();
    let derived_ipfs_id = get_lit_action_ipfs_id(code_to_run.clone()).await?;

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

    tracing::info!("Signed data: {:?}", result.signed_data);

    let signatures = result
        .signed_data
        .iter()
        .map(|(name, signed_data)| LitActionSignature {
            name: name.clone(),
            data: signed_data.clone().into(),
        })
        .collect();

    let lit_action_response = LitActionResponse {
        signatures,
        response: result.response,
        logs: result.logs,
        has_error: false,
    };

    Ok(lit_action_response)
}

async fn get_lit_action_ipfs_id(code: String) -> Result<String, ApiStatus> {
    let ipfs_hasher = IpfsHasher::default();
    let derived_ipfs_id = ipfs_hasher.compute(code.as_bytes());
    Ok(derived_ipfs_id)
}
