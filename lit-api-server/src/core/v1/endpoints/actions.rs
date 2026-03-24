use std::sync::Arc;

use crate::accounts::chain_config::ChainConfig;
use crate::actions::grpc::GrpcClientPool;
use crate::core::core_features;
use crate::core::v1::guards::billing::BilledLitActionApiKey;
use crate::core::v1::guards::cpu_overload::CpuAvailable;
use crate::core::v1::helpers::api_status::{ApiResult, ErrMessage};
use crate::core::v1::helpers::open_api_response::OpenApiResponse;
use crate::core::v1::models::request::LitActionRequest;
use crate::core::v1::models::response::LitActionResponse;
use crate::observability::RequestSpan;
use moka::future::Cache;
use rocket::State;
use rocket::post;
use rocket::serde::json::Json;
use rocket_okapi::openapi;

#[openapi(tag = "Actions")]
#[post("/lit_action", format = "json", data = "<lit_action_request>")]
#[tracing::instrument(name = "endpoint::lit_action", skip_all, parent = &request_span.span)]
#[allow(clippy::too_many_arguments)]
pub(super) async fn lit_action(
    _cpu: CpuAvailable,
    request_span: RequestSpan,
    api_key: BilledLitActionApiKey,
    grpc_client_pool: &State<GrpcClientPool<tonic::transport::Channel>>,
    ipfs_cache: &State<Cache<String, String>>,
    http_client: &State<reqwest::Client>,
    chain_config: &State<Arc<ChainConfig>>,
    lit_action_request: Json<LitActionRequest>,
) -> OpenApiResponse<LitActionResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            core_features::lit_action(
                &request_span,
                api_key.0.as_str(),
                grpc_client_pool.inner(),
                ipfs_cache.inner(),
                http_client.inner(),
                chain_config.inner().clone(),
                lit_action_request,
            )
            .await,
        )
        .into(),
    }
}
