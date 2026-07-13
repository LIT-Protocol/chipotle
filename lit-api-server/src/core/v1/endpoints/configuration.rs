use std::sync::Arc;

use crate::accounts::chain_config::ChainConfig;
use crate::actions::languages::SupportedLanguages;
use crate::core::account_management;
use crate::core::core_features;
use crate::core::v1::helpers::api_status::ApiResult;
use crate::core::v1::helpers::api_status::ErrMessage;
use crate::core::v1::helpers::open_api_response::OpenApiResponse;
use crate::core::v1::models::response::LitActionClientConfigResponse;
use crate::core::v1::models::response::SupportedLanguagesResponse;
use crate::core::v1::models::response::VersionResponse;
use rocket::State;
use rocket::get;
use rocket_okapi::openapi;

#[openapi(tag = "Configuration")]
#[get("/get_lit_action_client_config")]
pub(super) async fn get_lit_action_client_config(
    chain_config: &State<Arc<ChainConfig>>,
) -> OpenApiResponse<LitActionClientConfigResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            core_features::get_lit_action_client_config(chain_config.inner().clone()).await,
        )
        .into(),
    }
}

/// Advertises the node's language capability surface: which languages,
/// runtimes, and execution methods this node admits. No guards — like
/// `get_lit_action_client_config`, it exists so clients can discover
/// capability before uploading anything.
#[openapi(tag = "Configuration")]
#[get("/get_supported_languages")]
pub(super) async fn get_supported_languages(
    languages: &State<Arc<SupportedLanguages>>,
) -> OpenApiResponse<SupportedLanguagesResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(Ok(SupportedLanguagesResponse {
            languages: languages.languages().to_vec(),
        }))
        .into(),
    }
}

#[openapi(tag = "Configuration")]
#[get("/get_api_payers")]
pub(super) async fn get_api_payers() -> OpenApiResponse<Vec<String>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::get_api_payers().await).into(),
    }
}

#[openapi(tag = "Configuration")]
#[get("/get_admin_api_payer")]
pub(super) async fn get_admin_api_payer() -> OpenApiResponse<String, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::get_admin_api_payer().await).into(),
    }
}

#[openapi(tag = "Configuration")]
#[get("/version")]
pub(super) async fn get_version() -> OpenApiResponse<VersionResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(Ok(VersionResponse {
            version: crate::version::CARGO_PKG_VERSION.to_string(),
            commit_version: crate::version::GIT_VERSION.to_string(),
            name: crate::version::CARGO_PKG_NAME.to_string(),
            submodule_versions: crate::version::GIT_SUBMODULE_VERSIONS
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }))
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::{ContentType, Status};
    use rocket::local::blocking::Client;
    use rocket::routes;

    #[test]
    fn get_supported_languages_serves_the_managed_allowlist() {
        let languages = SupportedLanguages::parse(
            "javascript|raw_script; python:python3.13:python3.12|raw_script,bundle",
        )
        .expect("valid allowlist");
        let rocket = rocket::build()
            .mount("/", routes![get_supported_languages])
            .manage(Arc::new(languages));
        let client = Client::tracked(rocket).expect("valid rocket");

        let resp = client.get("/get_supported_languages").dispatch();
        assert_eq!(resp.status(), Status::Ok);
        assert_eq!(resp.content_type(), Some(ContentType::JSON));

        let body: SupportedLanguagesResponse =
            serde_json::from_str(&resp.into_string().expect("body")).expect("valid response JSON");
        assert_eq!(body.languages.len(), 2);
        assert_eq!(body.languages[0].name, "javascript");
        assert_eq!(body.languages[1].name, "python");
        assert_eq!(body.languages[1].runtimes[0].id, "python3.13");
        assert!(body.languages[1].runtimes[0].is_default);
    }
}
