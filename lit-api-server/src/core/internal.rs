use crate::accounts;
use crate::actions::action_client::DenoExecutionEnv;
use crate::actions:: action_client;
use lit_core::utils::binary::bytes_to_hex;
use lit_rust_crypto::group::GroupEncoding;
use lit_rust_crypto::k256::SecretKey;
use moka::future::Cache;
use rand::Rng;
use crate::actions::grpc_client_pool::GrpcClientPool;
use crate::core::models::ApiStatus;
use crate::core::v1::models::request::{
    LitActionRequest, SignWithPKPRequest,    
};
use crate::core::v1::models::response::{
    CreateWalletResponse, GetApiKeyResponse, LitActionResponse, SignWithPkpResponse
};
use rocket::serde::json::Json;

fn not_configured() -> ApiStatus {
    ApiStatus::internal_server_error(
        anyhow::anyhow!("Lit testnet not configured"),
        "This operation is not available.",
    )
}

fn get_random_secret() -> [u8; 32] {
    let mut secret: [u8; 32] = [0; 32];

    // Get a thread-local random number generator and fill the array.
    rand::thread_rng().fill(&mut secret);
    secret
}

fn get_new_wallet() -> (String, String, [u8; 32]) {
    let secret = get_random_secret();
    let secret_key = SecretKey::from_slice(&secret).unwrap();
    let public_key = secret_key.public_key();
    let public_key_bytes = public_key.as_affine().to_bytes();
    let wallet_address = bytes_to_hex(&public_key_bytes);
    let public_key = bytes_to_hex(&public_key_bytes);
    (public_key, wallet_address, secret)
}

pub async fn get_api_key() -> Result<GetApiKeyResponse, ApiStatus> {
    let (_public_key, wallet_address, secret) = get_new_wallet();
    let api_key = base64_light::base64_encode_bytes(&secret);

    if let Err(e) = accounts::new_account(&api_key).await {
        return Err(e.into());
    }

    Ok(GetApiKeyResponse {
        api_key: api_key,
        wallet_address: wallet_address,
    })
}

pub async fn create_wallet(api_key: &str) -> Result<CreateWalletResponse, ApiStatus> {
    let (_public_key, wallet_address, _secret) = get_new_wallet();
    Ok(CreateWalletResponse {
        wallet_address: wallet_address,
    })
}

pub async fn sign_with_pkp(sign_request: Json<SignWithPKPRequest>) -> Result<SignWithPkpResponse, ApiStatus> {
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

    let request_id = Some( "test".to_string());

    let deno_execution_env = DenoExecutionEnv {
        ipfs_cache: Some(moka::future::Cache::clone(ipfs_cache)),
        http_client: Some(reqwest::Client::clone(http_client)),
    };


    let mut client = match action_client::ClientBuilder::default()
        .js_env(deno_execution_env)
        .request_id(request_id.clone())
        // .http_headers(http_headers)
        .client_grpc_channels((*grpc_client_pool).clone())        
        // .key_set_id(json_execution_request.key_set_id.clone())
        .build()
        .map_err(|e| {
            e.to_string()
        }) {
        Ok(client) => client,
        Err(e) => {
            return Err(anyhow::anyhow!("failed to build client: {:?}", e).into())
        }
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

    let result = match  client.execute_js(execution_options).await {
        Ok(result) => result, 
        Err(e) => {
            return Err(anyhow::anyhow!("Actions failed with : {:?}", e).into())
        }
    };

    let lit_action_response = LitActionResponse {
        signatures: vec![],
        response: result.response, 
        logs: result.logs,
        has_error: false,
    };
  
    Ok(lit_action_response)
}

