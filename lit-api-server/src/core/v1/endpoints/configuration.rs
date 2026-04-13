use std::sync::Arc;

use crate::accounts::chain_config::ChainConfig;
use crate::core::account_management;
use crate::core::core_features;
use crate::core::v1::helpers::api_status::ApiResult;
use crate::core::v1::helpers::api_status::ErrMessage;
use crate::core::v1::helpers::open_api_response::OpenApiResponse;
use crate::core::v1::models::response::LitActionClientConfigResponse;
use crate::core::v1::models::response::VersionResponse;
use rocket::State;
use rocket::get;
use rocket_okapi::openapi;

#[openapi(tag = "Configuration")]
#[get("/get_lit_action_client_config")]
pub(super) async fn get_lit_action_client_config(
    chain_config: &State<Arc<ChainConfig>>,
) -> OpenApiResponse<LitActionClientConfigResponse, ErrMessage> {
    OpenApiResponse::new(
        ApiResult(
            core_features::get_lit_action_client_config(chain_config.inner().clone()).await,
        )
        .into(),
    )
}

#[openapi(tag = "Configuration")]
#[get("/get_api_payers")]
pub(super) async fn get_api_payers() -> OpenApiResponse<Vec<String>, ErrMessage> {
    OpenApiResponse::new(ApiResult(account_management::get_api_payers().await).into())
}

#[openapi(tag = "Configuration")]
#[get("/get_admin_api_payer")]
pub(super) async fn get_admin_api_payer() -> OpenApiResponse<String, ErrMessage> {
    OpenApiResponse::new(ApiResult(account_management::get_admin_api_payer().await).into())
}

#[openapi(tag = "Configuration")]
#[get("/version")]
pub(super) async fn get_version() -> OpenApiResponse<VersionResponse, ErrMessage> {
    OpenApiResponse::new(
        ApiResult(Ok(VersionResponse {
            version: crate::version::CARGO_PKG_VERSION.to_string(),
            commit_version: crate::version::GIT_VERSION.to_string(),
            name: crate::version::CARGO_PKG_NAME.to_string(),
            submodule_versions: crate::version::GIT_SUBMODULE_VERSIONS
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }))
        .into(),
    )
}
