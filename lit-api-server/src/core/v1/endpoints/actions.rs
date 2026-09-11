use std::sync::Arc;

use crate::accounts::chain_config::ChainConfig;
use crate::actions::grpc::GrpcClientPool;
use crate::actions::gvisor::GvisorEnabled;
use crate::core::cache_metadata::CacheMetadataIndex;
use crate::core::core_features;
use crate::core::v1::guards::billing::BilledLitActionApiKey;
use crate::core::v1::guards::cpu_overload::CpuAvailable;
use crate::core::v1::health::LitActionsGvisorSocketPath;
use crate::core::v1::helpers::api_status::{ApiResult, ErrMessage};
use crate::core::v1::helpers::open_api_response::OpenApiResponse;
use crate::core::v1::models::request::{LitActionRequest, LitBinaryActionRequest};
use crate::core::v1::models::response::LitActionResponse;
use crate::observability::RequestSpan;
use crate::stripe::StripeState;
use moka::future::Cache;
use rocket::State;
use rocket::serde::json::Json;
use rocket::{get, post};
use rocket_okapi::openapi;

/// Discover an action's public identity directly over authenticated Lit TLS.
/// Allows clients to verify action-signed encrypted results without trusting
/// an application proxy's claimed public key. No API key or billing required.
#[openapi(tag = "Actions")]
#[get("/lit_action_public_key/<cid>")]
pub(super) async fn lit_action_public_key(
    _cpu: CpuAvailable,
    cid: &str,
) -> OpenApiResponse<crate::core::v1::models::response::LitActionPublicKeyResponse, ErrMessage> {
    use crate::core::v1::helpers::api_status::ApiStatus;
    use crate::core::v1::models::response::LitActionPublicKeyResponse;
    use std::{sync::OnceLock, time::Duration};
    static KEYS: OnceLock<Cache<String, String>> = OnceLock::new();
    static IN_FLIGHT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(16);
    let result = async {
        if cid.len() != 46
            || !cid.starts_with("Qm")
            || !cid
                .bytes()
                .all(|c| b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&c))
        {
            return Err(ApiStatus::bad_request(
                anyhow::anyhow!("invalid CID"),
                "expected a CIDv0 action identifier",
            ));
        }
        let keys = KEYS.get_or_init(|| {
            Cache::builder()
                .max_capacity(10000)
                .time_to_live(Duration::from_secs(300))
                .build()
        });
        if let Some(public_key) = keys.get(cid).await {
            return Ok(LitActionPublicKeyResponse { public_key });
        }
        let _permit = IN_FLIGHT.try_acquire().map_err(|_| {
            ApiStatus::internal_server_error(
                anyhow::anyhow!("busy"),
                "public key discovery unavailable",
            )
        })?;
        let public_key = tokio::time::timeout(
            Duration::from_secs(10),
            crate::actions::client::op_code_helpers::private_keys::get_lit_action_public_key(cid),
        )
        .await
        .map_err(|_| {
            ApiStatus::internal_server_error(
                anyhow::anyhow!("timeout"),
                "public key discovery unavailable",
            )
        })?
        .map_err(|_| {
            ApiStatus::internal_server_error(
                anyhow::anyhow!("derivation failed"),
                "public key discovery unavailable",
            )
        })?;
        keys.insert(cid.to_owned(), public_key.clone()).await;
        Ok(LitActionPublicKeyResponse { public_key })
    }
    .await;
    OpenApiResponse {
        response: ApiResult(result).into(),
    }
}

#[openapi(tag = "Actions")]
#[post("/lit_action", format = "json", data = "<lit_action_request>")]
#[tracing::instrument(name = "endpoint::lit_action", skip_all, parent = &request_span.span)]
#[allow(clippy::too_many_arguments)]
pub(super) async fn lit_action(
    _cpu: CpuAvailable,
    request_span: RequestSpan,
    api_key: BilledLitActionApiKey,
    grpc_client_pool: &State<GrpcClientPool<tonic::transport::Channel>>,
    ipfs_cache: &State<Cache<String, Arc<String>>>,
    cache_metadata: &State<Arc<CacheMetadataIndex>>,
    http_client: &State<reqwest::Client>,
    chain_config: &State<Arc<ChainConfig>>,
    stripe_state: &State<Option<Arc<StripeState>>>,
    lit_action_request: Json<LitActionRequest>,
) -> OpenApiResponse<LitActionResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            core_features::lit_action(
                &request_span,
                api_key.0.as_str(),
                grpc_client_pool.inner(),
                ipfs_cache.inner(),
                cache_metadata.inner().clone(),
                http_client.inner(),
                chain_config.inner().clone(),
                stripe_state.inner().clone(),
                lit_action_request,
            )
            .await,
        )
        .into(),
    }
}

/// Execute an any-language action bundle on the gVisor runner. Same billing,
/// CPU-gating, and response shape as `/lit_action`; differs only in payload
/// (a tar bundle instead of JS) and backend socket. The sandbox always runs
/// `bash startup.sh` — the request's `startup_script`, or the bundle's root
/// `startup.sh` — so one cached bundle serves many different scripts, and
/// top-level `js_params` are injected as environment variables.
#[openapi(tag = "Actions")]
#[post("/lit_binary_action", format = "json", data = "<request>")]
#[tracing::instrument(name = "endpoint::lit_binary_action", skip_all, parent = &request_span.span)]
#[allow(clippy::too_many_arguments)]
pub(super) async fn lit_binary_action(
    // First guard: gVisor is off by default (CPL-359). When disabled this
    // short-circuits with a "feature disabled" 503 before the CPU and billing
    // guards run, so a disabled node never reaches the Stripe credit check.
    _gvisor: GvisorEnabled,
    _cpu: CpuAvailable,
    request_span: RequestSpan,
    api_key: BilledLitActionApiKey,
    grpc_client_pool: &State<GrpcClientPool<tonic::transport::Channel>>,
    ipfs_cache: &State<Cache<String, Arc<String>>>,
    http_client: &State<reqwest::Client>,
    chain_config: &State<Arc<ChainConfig>>,
    stripe_state: &State<Option<Arc<StripeState>>>,
    gvisor_socket: &State<LitActionsGvisorSocketPath>,
    request: Json<LitBinaryActionRequest>,
) -> OpenApiResponse<LitActionResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            core_features::lit_binary_action(
                &request_span,
                api_key.0.as_str(),
                grpc_client_pool.inner(),
                ipfs_cache.inner(),
                http_client.inner(),
                chain_config.inner().clone(),
                stripe_state.inner().clone(),
                gvisor_socket.0.clone(),
                request,
            )
            .await,
        )
        .into(),
    }
}
