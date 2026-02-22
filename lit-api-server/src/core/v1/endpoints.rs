use crate::actions::grpc::GrpcClientPool;
use crate::core::account_management;
use crate::core::core_features;
use crate::core::api_status::ApiResult;
use crate::core::api_status::ErrMessage;
use crate::core::v1::models::request::{
    AddActionToGroupRequest, AddGroupRequest, AddPkpToGroupRequest, AddUsageApiKeyRequest,
    LitActionRequest, NewAccountRequest, RemoveActionFromGroupRequest, RemovePkpFromGroupRequest,
    RemoveUsageApiKeyRequest, SignWithPKPRequest, UpdateActionMetadataRequest, UpdateGroupRequest,
    UpdateUsageApiKeyMetadataRequest,
};
use crate::core::v1::models::response::ApiKeyItem;
use crate::core::v1::models::response::WalletItem;
use crate::core::v1::models::response::{
    AccountOpResponse, CreateWalletResponse, ListMetadataItem, LitActionResponse,
    NewAccountResponse, SignWithPkpResponse, NodeChainConfigResponse, AddUsageApiKeyResponse
};
use moka::future::Cache;
use rocket::Route;
use rocket::State;
use rocket::response::Responder;
use rocket::serde::json::Json;
use rocket::{get, post};
use rocket_okapi::OpenApiError;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::okapi::openapi3::OpenApi;
use rocket_okapi::openapi;
use rocket_okapi::openapi_get_routes_spec;
use rocket_okapi::response::OpenApiResponderInner;
use rocket_responder::ApiResponse;
use schemars::JsonSchema;
use serde::Serialize;

struct OpenApiResponse<T: Serialize + JsonSchema, E: Serialize + JsonSchema> {
    response: ApiResponse<T, E>,
}

impl<T: Serialize + JsonSchema, E: Serialize + JsonSchema> OpenApiResponderInner
    for OpenApiResponse<T, E>
{
    fn responses(
        generator: &mut OpenApiGenerator,
    ) -> std::result::Result<rocket_okapi::okapi::openapi3::Responses, OpenApiError> {
        let mut responses = rocket_okapi::okapi::openapi3::Responses::default();
        let schema = generator.json_schema::<T>();
        rocket_okapi::util::add_default_response_schema(
            &mut responses,
            "application/json".to_string(),
            schema,
        );
        Ok(responses)
    }
}

impl<'r, T: Serialize + JsonSchema, E: Serialize + JsonSchema> Responder<'r, 'static>
    for OpenApiResponse<T, E>
{
    fn respond_to(self, request: &'r rocket::Request<'_>) -> rocket::response::Result<'static> {
        self.response.respond_to(request)
    }
}

/// Returns Core v1 routes and the OpenAPI spec for them. Mount routes at `/core/v1/` and serve the spec at `/openapi.json` (or use it for Swagger UI).
pub fn routes_with_spec() -> (Vec<Route>, OpenApi) {
    openapi_get_routes_spec![
        // sign_with_pkp,
        list_api_keys,
        new_account,
        account_exists,
        create_wallet,
        lit_action,
        get_lit_action_ipfs_id,
        add_group,
        add_action_to_group,
        add_pkp_to_group,
        remove_pkp_from_group,
        add_usage_api_key,
        remove_usage_api_key,
        update_group,
        remove_action_from_group,
        update_action_metadata,
        update_usage_api_key_metadata,
        list_groups,
        list_wallets,
        list_wallets_in_group,
        list_actions,
        get_node_chain_config,
    ]
}

#[openapi(tag = "Account Management")]
#[post("/new_account", format = "json", data = "<new_account_request>")]
async fn new_account(
    new_account_request: Json<NewAccountRequest>,
) -> OpenApiResponse<NewAccountResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::new_account(new_account_request).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/account_exists/<api_key>")]
async fn account_exists(api_key: String) -> OpenApiResponse<bool, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::account_exists(api_key.as_str()).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/create_wallet/<api_key>")]
async fn create_wallet(api_key: &str) -> OpenApiResponse<CreateWalletResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::create_wallet(api_key).await).into(),
    }
}

#[openapi(tag = "Signing")]
#[post("/sign_with_pkp", format = "json", data = "<sign_request>")]
async fn sign_with_pkp(
    sign_request: Json<SignWithPKPRequest>,
) -> OpenApiResponse<SignWithPkpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(core_features::sign_with_pkp(sign_request).await).into(),
    }
}

#[openapi(tag = "Actions")]
#[post("/lit_action", format = "json", data = "<lit_action_request>")]
async fn lit_action(
    grpc_client_pool: &State<GrpcClientPool<tonic::transport::Channel>>,
    ipfs_cache: &State<Cache<String, String>>,
    // action_store: &State<ActionStore>,
    http_client: &State<reqwest::Client>,
    lit_action_request: Json<LitActionRequest>,
) -> OpenApiResponse<LitActionResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            core_features::lit_action(
                grpc_client_pool.inner(),
                ipfs_cache.inner(),
                http_client.inner(),
                lit_action_request,
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/get_lit_action_ipfs_id/<code>")]
async fn get_lit_action_ipfs_id(code: String) -> OpenApiResponse<String, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::get_lit_action_ipfs_id(code).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_group", format = "json", data = "<req>")]
async fn add_group(req: Json<AddGroupRequest>) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::add_group(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_action_to_group", format = "json", data = "<req>")]
async fn add_action_to_group(
    req: Json<AddActionToGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::add_action_to_group(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_pkp_to_group", format = "json", data = "<req>")]
async fn add_pkp_to_group(
    req: Json<AddPkpToGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::add_pkp_to_group(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/remove_pkp_from_group", format = "json", data = "<req>")]
async fn remove_pkp_from_group(
    req: Json<RemovePkpFromGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::remove_pkp_from_group(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_usage_api_key", format = "json", data = "<req>")]
async fn add_usage_api_key(
    req: Json<AddUsageApiKeyRequest>,
) -> OpenApiResponse<AddUsageApiKeyResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::add_usage_api_key(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/remove_usage_api_key", format = "json", data = "<req>")]
async fn remove_usage_api_key(
    req: Json<RemoveUsageApiKeyRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::remove_usage_api_key(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/update_group", format = "json", data = "<req>")]
async fn update_group(
    req: Json<UpdateGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::update_group(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/remove_action_from_group", format = "json", data = "<req>")]
async fn remove_action_from_group(
    req: Json<RemoveActionFromGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::remove_action_from_group(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/update_action_metadata", format = "json", data = "<req>")]
async fn update_action_metadata(
    req: Json<UpdateActionMetadataRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::update_action_metadata(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/update_usage_api_key_metadata", format = "json", data = "<req>")]
async fn update_usage_api_key_metadata(
    req: Json<UpdateUsageApiKeyMetadataRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::update_usage_api_key_metadata(req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_api_keys?<api_key>&<page_number>&<page_size>")]
async fn list_api_keys(
    api_key: String,
    page_number: String,
    page_size: String,
) -> OpenApiResponse<Vec<ApiKeyItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::list_api_keys(api_key.as_str(), page_number.as_str(), page_size.as_str()).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_groups?<api_key>&<page_number>&<page_size>")]
async fn list_groups(
    api_key: String,
    page_number: String,
    page_size: String,
) -> OpenApiResponse<Vec<ListMetadataItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_groups(api_key.as_str(), page_number.as_str(), page_size.as_str()).await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_wallets?<api_key>&<page_number>&<page_size>")]
async fn list_wallets(
    api_key: String,
    page_number: String,
    page_size: String,
) -> OpenApiResponse<Vec<WalletItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_wallets(api_key.as_str(), page_number.as_str(), page_size.as_str())
                .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_wallets_in_group?<api_key>&<group_id>&<page_number>&<page_size>")]
async fn list_wallets_in_group(
    api_key: String,
    group_id: String,
    page_number: String,
    page_size: String,
) -> OpenApiResponse<Vec<WalletItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_wallets_in_group(
                api_key.as_str(),
                group_id.as_str(),
                page_number.as_str(),
                page_size.as_str(),
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_actions?<api_key>&<group_id>&<page_number>&<page_size>")]
async fn list_actions(
    api_key: String,
    group_id: String,
    page_number: String,
    page_size: String,
) -> OpenApiResponse<Vec<ListMetadataItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_actions(
                api_key.as_str(),
                group_id.as_str(),
                page_number.as_str(),
                page_size.as_str(),
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/get_node_chain_config")]
async fn get_node_chain_config() -> OpenApiResponse<NodeChainConfigResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::get_chain_info().await).into(),
    }
}
