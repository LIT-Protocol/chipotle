use crate::actions::grpc::GrpcClientPool;
use crate::core::account_management;
use crate::core::api_status::ApiResult;
use crate::core::api_status::ErrMessage;
use crate::core::core_features;
use crate::core::v1::models::request::{
    AddActionToGroupRequest, AddGroupRequest, AddPkpToGroupRequest, AddUsageApiKeyRequest,
    LitActionRequest, NewAccountRequest, RemoveActionFromGroupRequest, RemovePkpFromGroupRequest,
    RemoveUsageApiKeyRequest, SignWithPKPRequest, UpdateActionMetadataRequest, UpdateGroupRequest,
    UpdateUsageApiKeyMetadataRequest,
};
use crate::core::v1::models::response::ApiKeyItem;
use crate::core::v1::models::response::WalletItem;
use crate::core::v1::models::response::{
    AccountOpResponse, AddUsageApiKeyResponse, CreateWalletResponse, ListMetadataItem,
    LitActionResponse, NewAccountResponse, NodeChainConfigResponse, SignWithPkpResponse,
};
use moka::future::Cache;
use rocket::Route;
use rocket::State;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::response::Responder;
use rocket::serde::json::Json;
use rocket::{get, post};
use rocket_okapi::OpenApiError;
use rocket_okapi::Result as RocketOkapiResult;
use rocket_okapi::r#gen::OpenApiGenerator;
use rocket_okapi::okapi::openapi3::OpenApi;
use rocket_okapi::okapi::openapi3::{Object, Parameter, ParameterValue};
use rocket_okapi::openapi;
use rocket_okapi::openapi_get_routes_spec;
use rocket_okapi::request::{OpenApiFromRequest, RequestHeaderInput};
use rocket_okapi::response::OpenApiResponderInner;
use rocket_responder::ApiResponse;
use schemars::JsonSchema;
use serde::Serialize;

/// Request guard that extracts the API key from `Authorization: Bearer <key>` or `X-Api-Key: <key>`.
#[derive(Clone, Debug)]
pub struct ApiKey(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ApiKey {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<ApiKey, Self::Error> {
        let auth = request.headers().get_one("Authorization");
        if let Some(v) = auth {
            let v = v.trim();
            if v.starts_with("Bearer ") {
                let key = v[7..].trim();
                if !key.is_empty() {
                    return Outcome::Success(ApiKey(key.to_string()));
                }
            }
        }
        if let Some(key) = request.headers().get_one("X-Api-Key") {
            let key = key.trim();
            if !key.is_empty() {
                return Outcome::Success(ApiKey(key.to_string()));
            }
        }
        Outcome::Error((Status::Unauthorized, ()))
    }
}

impl<'r> OpenApiFromRequest<'r> for ApiKey {
    fn from_request_input(
        generator: &mut OpenApiGenerator,
        _name: String,
        required: bool,
    ) -> RocketOkapiResult<RequestHeaderInput> {
        let schema = generator.json_schema::<String>();
        Ok(RequestHeaderInput::Parameter(Parameter {
            name: "X-Api-Key".to_owned(),
            location: "header".to_owned(),
            description: Some(
                "Account or usage API key. Alternatively use Authorization: Bearer <key>."
                    .to_owned(),
            ),
            required,
            deprecated: false,
            allow_empty_value: false,
            value: ParameterValue::Schema {
                style: None,
                explode: None,
                allow_reserved: false,
                schema,
                example: None,
                examples: None,
            },
            extensions: Object::default(),
        }))
    }
}

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
#[get("/account_exists")]
async fn account_exists(api_key: ApiKey) -> OpenApiResponse<bool, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::account_exists(api_key.0.as_str()).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/create_wallet")]
async fn create_wallet(api_key: ApiKey) -> OpenApiResponse<CreateWalletResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::create_wallet(api_key.0.as_str()).await).into(),
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
    api_key: ApiKey,
    grpc_client_pool: &State<GrpcClientPool<tonic::transport::Channel>>,
    ipfs_cache: &State<Cache<String, String>>,
    http_client: &State<reqwest::Client>,
    lit_action_request: Json<LitActionRequest>,
) -> OpenApiResponse<LitActionResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            core_features::lit_action(
                api_key.0.as_str(),
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
async fn add_group(
    api_key: ApiKey,
    req: Json<AddGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::add_group(api_key.0.as_str(), req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_action_to_group", format = "json", data = "<req>")]
async fn add_action_to_group(
    api_key: ApiKey,
    req: Json<AddActionToGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::add_action_to_group(api_key.0.as_str(), req).await)
            .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_pkp_to_group", format = "json", data = "<req>")]
async fn add_pkp_to_group(
    api_key: ApiKey,
    req: Json<AddPkpToGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::add_pkp_to_group(api_key.0.as_str(), req).await)
            .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/remove_pkp_from_group", format = "json", data = "<req>")]
async fn remove_pkp_from_group(
    api_key: ApiKey,
    req: Json<RemovePkpFromGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::remove_pkp_from_group(api_key.0.as_str(), req).await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/add_usage_api_key", format = "json", data = "<req>")]
async fn add_usage_api_key(
    api_key: ApiKey,
    req: Json<AddUsageApiKeyRequest>,
) -> OpenApiResponse<AddUsageApiKeyResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::add_usage_api_key(api_key.0.as_str(), req).await)
            .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/remove_usage_api_key", format = "json", data = "<req>")]
async fn remove_usage_api_key(
    api_key: ApiKey,
    req: Json<RemoveUsageApiKeyRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::remove_usage_api_key(api_key.0.as_str(), req).await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/update_group", format = "json", data = "<req>")]
async fn update_group(
    api_key: ApiKey,
    req: Json<UpdateGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::update_group(api_key.0.as_str(), req).await).into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/remove_action_from_group", format = "json", data = "<req>")]
async fn remove_action_from_group(
    api_key: ApiKey,
    req: Json<RemoveActionFromGroupRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::remove_action_from_group(api_key.0.as_str(), req).await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/update_action_metadata", format = "json", data = "<req>")]
async fn update_action_metadata(
    api_key: ApiKey,
    req: Json<UpdateActionMetadataRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::update_action_metadata(api_key.0.as_str(), req).await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[post("/update_usage_api_key_metadata", format = "json", data = "<req>")]
async fn update_usage_api_key_metadata(
    api_key: ApiKey,
    req: Json<UpdateUsageApiKeyMetadataRequest>,
) -> OpenApiResponse<AccountOpResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::update_usage_api_key_metadata(api_key.0.as_str(), req).await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_api_keys?<page_number>&<page_size>")]
async fn list_api_keys(
    api_key: ApiKey,
    page_number: String,
    page_size: String,
) -> OpenApiResponse<Vec<ApiKeyItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_api_keys(
                api_key.0.as_str(),
                page_number.as_str(),
                page_size.as_str(),
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_groups?<page_number>&<page_size>")]
async fn list_groups(
    api_key: ApiKey,
    page_number: String,
    page_size: String,
) -> OpenApiResponse<Vec<ListMetadataItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_groups(
                api_key.0.as_str(),
                page_number.as_str(),
                page_size.as_str(),
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_wallets?<page_number>&<page_size>")]
async fn list_wallets(
    api_key: ApiKey,
    page_number: String,
    page_size: String,
) -> OpenApiResponse<Vec<WalletItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_wallets(
                api_key.0.as_str(),
                page_number.as_str(),
                page_size.as_str(),
            )
            .await,
        )
        .into(),
    }
}

#[openapi(tag = "Account Management")]
#[get("/list_wallets_in_group?<group_id>&<page_number>&<page_size>")]
async fn list_wallets_in_group(
    api_key: ApiKey,
    group_id: String,
    page_number: String,
    page_size: String,
) -> OpenApiResponse<Vec<WalletItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_wallets_in_group(
                api_key.0.as_str(),
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
#[get("/list_actions?<group_id>&<page_number>&<page_size>")]
async fn list_actions(
    api_key: ApiKey,
    group_id: String,
    page_number: String,
    page_size: String,
) -> OpenApiResponse<Vec<ListMetadataItem>, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(
            account_management::list_actions(
                api_key.0.as_str(),
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
