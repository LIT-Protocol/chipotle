use crate::accounts;
use crate::actions::action_client;
use crate::actions::action_client::DenoExecutionEnv;
use crate::actions::grpc_client_pool::GrpcClientPool;
use crate::core::models::ApiStatus;
use crate::core::v1::models::request::{
    AddActionToGroupRequest, AddGroupRequest, AddPkpToGroupRequest, AddUsageApiKeyRequest,
    LitActionRequest, NewAccountRequest, RemovePkpFromGroupRequest, RemoveUsageApiKeyRequest,
    SignWithPKPRequest,
};
use crate::core::v1::models::response::{
    AccountOpResponse, CreateWalletResponse, ListMetadataItem, LitActionResponse,
    NewAccountResponse, SignWithPkpResponse,
};
use ethers::types::{H160, U256};
use ethers::utils::keccak256;
use lit_core::utils::binary::{bytes_to_hex, hex_to_bytes};
use lit_rust_crypto::group::GroupEncoding;
use lit_rust_crypto::k256::SecretKey;
use moka::future::Cache;
use rand::Rng;
use rocket::serde::json::Json;

fn not_configured() -> ApiStatus {
    ApiStatus::internal_server_error(
        anyhow::anyhow!("Lit testnet not configured"),
        "This operation is not available.",
    )
}

/// Parse U256 from decimal string or hex string (with or without 0x prefix).
fn parse_u256(s: &str) -> Result<U256, ApiStatus> {
    let s = s.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        let bytes = hex_to_bytes(s).map_err(|e| {
            ApiStatus::bad_request(anyhow::anyhow!(e), "invalid hex for U256")
        })?;
        Ok(U256::from_big_endian(&bytes))
    } else {
        U256::from_dec_str(s).map_err(|e| {
            ApiStatus::bad_request(anyhow::anyhow!(e), "invalid decimal for U256")
        })
    }
}

/// Parse vec of hex strings to Vec<U256> (for permitted_actions / pkps hashes).
fn parse_u256_hex_list(strings: &[String]) -> Result<Vec<U256>, ApiStatus> {
    strings
        .iter()
        .map(|s| {
            let bytes = hex_to_bytes(s.trim()).map_err(|e| {
                ApiStatus::bad_request(anyhow::anyhow!(e), "invalid hex in list")
            })?;
            Ok(U256::from_big_endian(&bytes))
        })
        .collect::<Result<Vec<_>, _>>()
}

fn get_random_secret() -> [u8; 32] {
    let mut secret: [u8; 32] = [0; 32];

    // Get a thread-local random number generator and fill the array.
    rand::thread_rng().fill(&mut secret);
    secret
}

fn get_new_wallet() -> (String, H160, [u8; 32]) {
    let secret = get_random_secret();
    let secret_key = SecretKey::from_slice(&secret).unwrap();
    let public_key = secret_key.public_key();
    let public_key_bytes = public_key.as_affine().to_bytes();

    let wallet_address = H160::from_slice(&keccak256(&public_key_bytes)[12..]);
    let public_key_string = bytes_to_hex(&public_key_bytes);
    (public_key_string, wallet_address, secret)
}

pub async fn new_account(
    new_account_request: Json<NewAccountRequest>,
) -> Result<NewAccountResponse, ApiStatus> {
    let account_name = new_account_request.account_name.clone();
    let account_description = new_account_request.account_description.clone();
    let (_public_key, wallet_address, secret) = get_new_wallet();
    let api_key = base64_light::base64_encode_bytes(&secret);

    if let Err(e) = accounts::new_account(
        &api_key,
        &account_name,
        &account_description,
        wallet_address,
    )
    .await
    {
        return Err(e.into());
    }

    Ok(NewAccountResponse {
        api_key: api_key,
        wallet_address: bytes_to_hex(&wallet_address.as_bytes()),
    })
}

pub async fn create_wallet(api_key: &str) -> Result<CreateWalletResponse, ApiStatus> {
    let (_public_key, wallet_address, secret) = get_new_wallet();

    let wallet_address_hex = bytes_to_hex(&wallet_address.as_bytes());

    // technically this is NOT a derivaton path at all, but it's a stand-in for now
    let derivation_path = U256::from_big_endian(&secret);
    if let Err(e) = accounts::register_wallet_derivation(&api_key, &wallet_address_hex, derivation_path, "", "").await {
        return Err(e.into());
    }

    Ok(CreateWalletResponse {
        wallet_address: bytes_to_hex(&wallet_address.as_bytes()),
    })
}

pub async fn sign_with_pkp(
    sign_request: Json<SignWithPKPRequest>,
) -> Result<SignWithPkpResponse, ApiStatus> {
    Err(not_configured())
}

pub async fn lit_action(
    grpc_client_pool: &GrpcClientPool<tonic::transport::Channel>,
    ipfs_cache: &Cache<String, String>,
    // action_store: &ActionStore,
    http_client: &reqwest::Client,
    // request_headers: RequestHeaders<'_>,
    lit_action_request: Json<LitActionRequest>,
) -> Result<LitActionResponse, ApiStatus> {
    let request_id = Some("test".to_string());

    let deno_execution_env = DenoExecutionEnv {
        ipfs_cache: Some(moka::future::Cache::clone(ipfs_cache)),
        http_client: Some(reqwest::Client::clone(http_client)),
    };

    let mut client = match action_client::ClientBuilder::default()
        .js_env(deno_execution_env)
        .request_id(request_id.clone())
        .api_key(lit_action_request.api_key.clone())
        // .http_headers(http_headers)
        .client_grpc_channels((*grpc_client_pool).clone())
        // .key_set_id(json_execution_request.key_set_id.clone())
        .build()
        .map_err(|e| e.to_string())
    {
        Ok(client) => client,
        Err(e) => return Err(anyhow::anyhow!("failed to build client: {:?}", e).into()),
    };

    let derived_ipfs_id = "some_Value".to_string();

    let code_to_run = lit_action_request.code.clone();
    let js_params = lit_action_request.js_params.clone();
    let execution_options = action_client::ExecutionOptions {
        code: code_to_run,
        globals: js_params.clone(),
        action_ipfs_id: Some(derived_ipfs_id),
    };

    // for async calls.
    // let result = match   client
    //     .execute_js_async(execution_options, action_store)
    //     .await
    //     {
    //         Ok(job_id) => {
    //             info!("Submitted async action job with ID {job_id}");
    //             client_session.json_encrypt_response_status::<JobId>(job_id)
    //         }
    //         Err(err) => {
    //             error!("Error processing async action job: {err:?}");
    //             return client_session.json_encrypt_err_custom_response(
    //                 "error processing async action job",
    //                 unexpected_err_code(
    //                     err,
    //                     EC::NodeJsExecutionError,
    //                     Some("Error processing action job".into()),
    //                 )
    //                 .handle(),
    //             );
    //         }
    //     };

    let result = match client.execute_js(execution_options).await {
        Ok(result) => result,
        Err(e) => return Err(anyhow::anyhow!("Actions failed with : {:?}", e).into()),
    };

    let lit_action_response = LitActionResponse {
        signatures: vec![],
        response: result.response,
        logs: result.logs,
        has_error: false,
    };

    Ok(lit_action_response)
}

pub async fn add_group(
    req: Json<AddGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let permitted_actions = parse_u256_hex_list(&req.permitted_actions)?;
    let pkps = parse_u256_hex_list(&req.pkps)?;
    accounts::add_group(
        &req.api_key,
        &req.group_name,
        &req.group_description,
        permitted_actions,
        pkps,
    )
    .await
    .map_err(|e| ApiStatus::internal_server_error(e, "add_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn add_action_to_group(
    req: Json<AddActionToGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = parse_u256(&req.group_id)?;
    let name = req.name.as_deref().unwrap_or("");
    let description = req.description.as_deref().unwrap_or("");
    accounts::add_action_to_group(&req.api_key, group_id, &req.action_ipfs_cid, name, description)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "add_action_to_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn add_pkp_to_group(
    req: Json<AddPkpToGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = parse_u256(&req.group_id)?;
    accounts::add_pkp_to_group(&req.api_key, group_id, &req.pkp_public_key)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "add_pkp_to_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn remove_pkp_from_group(
    req: Json<RemovePkpFromGroupRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let group_id = parse_u256(&req.group_id)?;
    accounts::remove_pkp_from_group(&req.api_key, group_id, &req.pkp_public_key)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "remove_pkp_from_group failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn add_usage_api_key(
    req: Json<AddUsageApiKeyRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    let expiration = parse_u256(&req.expiration)?;
    let balance = parse_u256(&req.balance)?;
    accounts::add_usage_api_key(&req.api_key, &req.usage_api_key, expiration, balance)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "add_usage_api_key failed"))?;
    Ok(AccountOpResponse { success: true })
}

pub async fn remove_usage_api_key(
    req: Json<RemoveUsageApiKeyRequest>,
) -> Result<AccountOpResponse, ApiStatus> {
    accounts::remove_usage_api_key(&req.api_key, &req.usage_api_key)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "remove_usage_api_key failed"))?;
    Ok(AccountOpResponse { success: true })
}

fn metadata_to_item(m: &accounts::Metadata) -> ListMetadataItem {
    ListMetadataItem {
        id: m.id.to_string(),
        name: m.name.clone(),
        description: m.description.clone(),
    }
}

pub async fn list_groups(
    api_key: &str,
    page_number: &str,
    page_size: &str,
) -> Result<Vec<ListMetadataItem>, ApiStatus> {
    let pn = parse_u256(page_number)?;
    let ps = parse_u256(page_size)?;
    let list = accounts::list_groups(api_key, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_groups failed"))?;
    Ok(list.iter().map(metadata_to_item).collect())
}

pub async fn list_wallets(
    api_key: &str,
    page_number: &str,
    page_size: &str,
) -> Result<Vec<ListMetadataItem>, ApiStatus> {
    let pn = parse_u256(page_number)?;
    let ps = parse_u256(page_size)?;
    let list = accounts::list_wallets(api_key, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_wallets failed"))?;
    Ok(list.iter().map(metadata_to_item).collect())
}

pub async fn list_wallets_in_group(
    api_key: &str,
    group_id: &str,
    page_number: &str,
    page_size: &str,
) -> Result<Vec<ListMetadataItem>, ApiStatus> {
    let gid = parse_u256(group_id)?;
    let pn = parse_u256(page_number)?;
    let ps = parse_u256(page_size)?;
    let list = accounts::list_wallets_in_group(api_key, gid, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_wallets_in_group failed"))?;
    Ok(list.iter().map(metadata_to_item).collect())
}

pub async fn list_actions(
    api_key: &str,
    group_id: &str,
    page_number: &str,
    page_size: &str,
) -> Result<Vec<ListMetadataItem>, ApiStatus> {
    let gid = parse_u256(group_id)?;
    let pn = parse_u256(page_number)?;
    let ps = parse_u256(page_size)?;
    let list = accounts::list_actions(api_key, gid, pn, ps)
        .await
        .map_err(|e| ApiStatus::internal_server_error(e, "list_actions failed"))?;
    Ok(list.iter().map(metadata_to_item).collect())
}
