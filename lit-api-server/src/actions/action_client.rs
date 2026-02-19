//! This module implements the gRPC client for the Lit Actions server (lit_actions).
//! The client initiates JS code execution and handles op requests from the server.
//! It holds all configuration data (including secrets) and manages state; none of
//! which are shared with lit_actions, enabling a secure execution environment.

use crate::actions::grpc_client_pool::GrpcClientPool;
use crate::error::{Error, unexpected_err};
use anyhow::{Context, Result, anyhow, bail};
use lit_core::utils::binary::bytes_to_hex;
use lit_rust_crypto::k256::SecretKey;
use lit_rust_crypto::k256::ecdsa::SigningKey;
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::actions::job::{ActionJob, ActionStore, JobId};
use derive_builder::Builder;
// use ecdsa::SignatureSize;
use futures::{FutureExt as _, TryFutureExt};
use lit_actions_grpc::tokio_stream::StreamExt as _;
use lit_actions_grpc::tonic::{
    Code, Extensions, Request, Status, metadata::MetadataMap, transport::Error as TransportError,
};

use lit_actions_grpc::{proto::*, unix};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use tokio::time::Duration;
use tracing::{instrument, trace};

const DEFAULT_TIMEOUT_MS: u64 = 30_000; // 30s
const DEFAULT_ASYNC_TIMEOUT_MS: u64 = 300_000; // 5m
const DEFAULT_CLIENT_TIMEOUT_MS_BUFFER: u64 = 5_000;
const DEFAULT_MEMORY_LIMIT_MB: u32 = 256; // 256MB

const DEFAULT_MAX_CODE_LENGTH: usize = 16 * 1024 * 1024; // 16MB
const DEFAULT_MAX_CONSOLE_LOG_LENGTH: usize = 1024 * 100; // 100KB
const DEFAULT_MAX_CONTRACT_CALL_COUNT: u32 = 30;
const DEFAULT_MAX_FETCH_COUNT: u32 = 50;
const DEFAULT_MAX_RESPONSE_LENGTH: usize = 1024 * 100; // 100KB
const DEFAULT_MAX_SIGN_COUNT: u32 = 10; // 10 signature requests per action execution
const DEFAULT_MAX_BROADCAST_AND_COLLECT_COUNT: u32 = 30;
const DEFAULT_MAX_CALL_DEPTH: u32 = 5;
const DEFAULT_MAX_RETRIES: u32 = 3;

#[derive(Debug, Clone, Default)]
pub struct DenoExecutionEnv {
    // pub tss_state: Option<crate::tss::common::tss_state::TssState>,
    // pub cfg: Arc<LitConfig>,
    pub ipfs_cache: Option<Cache<String, String>>,
    pub http_client: Option<reqwest::Client>,
}

#[derive(Debug, Default, Clone, Builder, Serialize, Deserialize)]
pub struct Client {
    #[builder(default, setter(into))]
    api_key: String,
    // Config
    #[builder(default, setter(into, strip_option))]
    socket_path: Option<PathBuf>,
    #[builder(default, setter(into))]
    #[serde(skip)]
    pub(crate) js_env: DenoExecutionEnv,
    // #[builder(default, setter(into))]
    // auth_context: models::AuthContext,
    // #[builder(default, setter(into))]
    // auth_sig: Option<JsonAuthSig>,
    #[builder(default, setter(into))]
    request_id: Option<String>,
    #[builder(default, setter(into))]
    http_headers: BTreeMap<String, String>,
    #[builder(default, setter(into))]
    epoch: Option<u64>,
    // #[builder(default, setter(into))]
    // endpoint_version: EndpointVersion,
    // #[builder(default, setter(into))]
    // node_set: Vec<NodeSet>,
    // #[builder(default, setter(into))]
    // key_set_id: String,
    // Limits
    #[builder(default = "DEFAULT_TIMEOUT_MS")]
    timeout_ms: u64,
    #[builder(default = "DEFAULT_ASYNC_TIMEOUT_MS")]
    #[serde(skip)]
    async_timeout_ms: u64,
    #[builder(default = "DEFAULT_MEMORY_LIMIT_MB")]
    memory_limit_mb: u32,
    #[builder(default = "DEFAULT_MAX_CODE_LENGTH")]
    max_code_length: usize,
    #[builder(default = "DEFAULT_MAX_RESPONSE_LENGTH")]
    max_response_length: usize,
    #[builder(default = "DEFAULT_MAX_CONSOLE_LOG_LENGTH")]
    max_console_log_length: usize,
    #[builder(default = "DEFAULT_MAX_FETCH_COUNT")]
    max_fetch_count: u32,
    #[builder(default = "DEFAULT_MAX_SIGN_COUNT")]
    max_sign_count: u32,
    #[builder(default = "DEFAULT_MAX_CONTRACT_CALL_COUNT")]
    max_contract_call_count: u32,
    #[builder(default = "DEFAULT_MAX_BROADCAST_AND_COLLECT_COUNT")]
    max_broadcast_and_collect_count: u32,
    #[builder(default = "DEFAULT_MAX_CALL_DEPTH")]
    max_call_depth: u32,
    #[builder(default = "DEFAULT_MAX_RETRIES")]
    max_retries: u32,

    #[builder(default)]
    #[serde(skip)]
    pub(crate) client_grpc_channels: GrpcClientPool<tonic::transport::Channel>,

    // #[builder(default)]
    // pub dynamic_payment: DynamicPayment,
    // State
    #[builder(setter(skip))]
    #[serde(skip)]
    pub(crate) state: ExecutionState,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionState {
    pub response: String,
    pub logs: String,
    #[serde(skip)]
    pub fetch_count: u32,
    #[serde(skip)]
    pub sign_count: u32,
    // pub signed_data: HashMap<String, SignedData>,
    #[serde(skip)]
    pub claim_count: u32,
    // pub claim_data: HashMap<String, response::JsonPKPClaimKeyResponse>,
    #[serde(skip)]
    pub contract_call_count: u32,
    #[serde(skip)]
    pub broadcast_and_collect_count: u32,
    #[serde(skip)]
    pub ops_count: u32,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ExecutionOptions {
    pub code: String,
    pub globals: Option<serde_json::Value>,
    pub action_ipfs_id: Option<String>,
}

impl From<&str> for ExecutionOptions {
    fn from(code: &str) -> Self {
        Self {
            code: code.to_string(),
            ..Default::default()
        }
    }
}

impl From<String> for ExecutionOptions {
    fn from(code: String) -> Self {
        Self {
            code: code,
            ..Default::default()
        }
    }
}

impl Client {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        ClientBuilder::default()
            .socket_path(socket_path)
            .build()
            .expect("cannot fail")
    }

    // fn lit_config(&self) -> &LitConfig {
    //     self.js_env.cfg.as_ref()
    // }

    fn socket_path(&self) -> PathBuf {
        self.socket_path.clone().unwrap_or_default()
    }

    fn client_timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms + DEFAULT_CLIENT_TIMEOUT_MS_BUFFER)
    }

    pub fn request_id(&self) -> String {
        self.request_id.clone().unwrap_or_default()
    }

    pub fn logs(&self) -> &str {
        &self.state.logs
    }

    // pub fn authorized_address(&self) -> Option<String> {
    //     self.auth_context
    //         .auth_sig_address
    //         .as_ref()
    //         .map(|s| s.to_lowercase())
    // }

    // fn tss_state_and_txn_prefix(&self) -> Result<(TssState, String)> {
    //     let tss_state = self
    //         .js_env
    //         .tss_state
    //         .clone()
    //         .expect_or_err("No TSS state found")?;
    //     let txn_prefix = self.request_id();
    //     Ok((tss_state, txn_prefix))
    // }

    fn ipfs_cache(&self) -> Result<Cache<String, String>> {
        // if let Some(ipfs_cache) = self.js_env.ipfs_cache.clone() {
        //     return Ok(ipfs_cache);
        // }

        bail!("No IPFS cache found");
    }

    fn http_cache(&self) -> Result<reqwest::Client> {
        if let Some(http_cache) = self.js_env.http_client.clone() {
            return Ok(http_cache);
        }
        bail!("No HTTP cache found");
    }

    fn metadata(&self) -> Result<MetadataMap> {
        let mut md = MetadataMap::new();
        // md.insert(
        //     "x-host",
        //     self.lit_config()
        //         .external_addr()
        //         .unwrap_or_default()
        //         .parse()?,
        // );
        md.insert("x-request-id", self.request_id().parse()?);

        // Add trace context to the metadata for distributed tracing.
        // inject_tracing_metadata(&mut md);

        Ok(md)
    }

    fn reset_state(&mut self) {
        std::mem::take(&mut self.state);
    }

    #[instrument(level = "debug", skip_all, ret)]
    pub async fn execute_js_async(
        &self,
        opts: impl Into<ExecutionOptions>,
        store: &ActionStore,
    ) -> Result<JobId> {
        store
            .clone()
            .submit_job(ActionJob::new(
                Client {
                    timeout_ms: self.async_timeout_ms,
                    ..self.clone()
                },
                opts,
            ))
            .await
    }

    #[instrument(level = "debug", skip_all, ret)]
    pub async fn execute_js(
        &mut self,
        opts: impl Into<ExecutionOptions>,
    ) -> Result<ExecutionState, Error> {
        self.reset_state();
        // self.dynamic_payment
        // .add(LitActionPriceComponent::BaseAmount, 1)?;
        let opts = opts.into();
        let timeout = self.client_timeout();

        // let auth_context = {
        //     let mut ctx = self.auth_context.clone();
        //     if let Some(id) = &opts.action_ipfs_id {
        //         ctx.action_ipfs_id_stack.push(id.clone());
        //     }
        //     ctx
        // };

        // Hand-roll retry loop as crates like tokio-retry or again don't play well with &mut self
        let mut retry = 0;
        loop {
            let execution = Box::pin(self.execute_js_inner(
                opts.code.clone(),
                opts.globals.clone(),
                // &auth_context,
                0,
            ));
            let execution_result = tokio::time::timeout(timeout, execution)
                .await
                .map_err(|e| {
                    unexpected_err(
                        e,
                        Some(format!("lit_actions didn't respond within {timeout:?}")),
                    )
                })?;
            match execution_result {
                Ok(state) => return Ok(state),
                Err(e) => {
                    let last_error = if let Some(status) = e.downcast_ref::<Status>() {
                        let msg = status.message().to_string();
                        match status.code() {
                            Code::DeadlineExceeded => {
                                return Err(unexpected_err(
                                    e,
                                    Some("tonic deadline exceeded error".to_string()),
                                ));
                            }
                            Code::ResourceExhausted => {
                                return Err(unexpected_err(
                                    e,
                                    Some("tonic resource exhausted error".to_string()),
                                ));
                            }
                            Code::Unavailable => {
                                // This error occurs when NGINX can't connect to any healthy
                                // lit_actions instance and returns the gRPC status code 14
                                return Err(unexpected_err(
                                    e,
                                    Some("tonic unavailable error".to_string()),
                                ));
                            }
                            // NB: We could also retry on `Code::Internal if msg == "h2 protocol error: error reading a body from connection"`.
                            // However, that likely means lit_actions has crashed *while* executing JS, which we can't recover from.
                            _ => return Err(unexpected_err(e, Some(msg.to_string()))),
                        }
                    } else if let Some(te) = e.downcast_ref::<TransportError>() {
                        // This error occurs when the socket file is missing or lit_actions is down
                        // - connection error: No such file or directory (os error 2)
                        // - connection error: Connection refused (os error 61)
                        anyhow!("tonic transport error: {:?}", te) // te.source().unwrap_or(te).to_string())
                    } else if let Some(se) = e.downcast_ref::<flume::SendError<ExecuteJsRequest>>()
                    {
                        // This error occurs when NGINX can't connect to any healthy lit_actions instance
                        // - connection error: sending on a closed channel
                        anyhow!(
                            "tonic send error: {:?}",
                            se // se.source().unwrap_or(se).to_string()
                        )
                    } else {
                        return Err(unexpected_err(e, None));
                    };

                    // Never retry in-flight requests, which may have modified state
                    if retry >= self.max_retries || self.state.ops_count != 0 {
                        return Err(unexpected_err(last_error, None));
                    }
                    let backoff = Duration::from_secs(2u64.pow(retry));
                    tracing::error!("Retrying execute_js in {backoff:?}, cause: {last_error:?}");
                    tokio::time::sleep(backoff).await;
                    retry += 1;
                }
            }
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    async fn execute_js_inner(
        &mut self,
        code: String,
        globals: Option<serde_json::Value>,
        // auth_context: &models::AuthContext,
        call_depth: u32,
    ) -> Result<ExecutionState> {
        if code.len() > self.max_code_length {
            bail!(
                "Code payload is too large ({} bytes). Max length is {} bytes.",
                code.len(),
                self.max_code_length,
            );
        }

        let (outbound_tx, outbound_rx) = flume::bounded(0);

        let socket_path = self.socket_path();
        let socket_path = PathBuf::from("/tmp/lit_actions.sock");
        let channel = self
            .client_grpc_channels
            .create_or_get_connection(&socket_path.display().to_string(), || {
                unix::connect_to_socket(socket_path)
                    .map_err(|e| anyhow!("Error creating connection to lit-action server: {:?}", e))
            })
            .await?;
        let mut stream = ActionClient::new(channel)
            .execute_js(Request::from_parts(
                self.metadata()?,
                Extensions::default(),
                outbound_rx.into_stream(),
            ))
            // Fix "implementation of `std::marker::Send` is not general enough"
            // Workaround for compiler bug https://github.com/rust-lang/rust/issues/96865
            // See https://github.com/rust-lang/rust/issues/100013#issuecomment-2052045872
            .boxed()
            .await?
            .into_inner();

        // Send initial execution request to server
        outbound_tx
            .send_async(
                ExecutionRequest {
                    code: code.to_string(),
                    js_params: globals.and_then(|v| serde_json::to_vec(&v).ok()),
                    auth_context: serde_json::to_vec(&[0; 0]).ok(),
                    http_headers: self.http_headers.clone(),
                    timeout: Some(self.timeout_ms),
                    memory_limit: Some(self.memory_limit_mb),
                }
                .into(),
            )
            .await
            .context("failed to send execution request")?;

        // Handle responses from server
        while let Some(resp) = stream.try_next().await? {
            match resp.union {
                // Return final result from server
                Some(UnionResponse::Result(res)) => {
                    if !res.success {
                        bail!(res.error);
                    }
                    // Return current state, which might be updated by subsequent code executions
                    return Ok(self.state.clone());
                }
                // Handle op requests
                Some(op) => {
                    let resp = self.handle_op(op, call_depth).await.unwrap_or_else(|e| {
                        ErrorResponse {
                            error: e.to_string(),
                        }
                        .into()
                    });
                    outbound_tx
                        .send_async(resp)
                        .await
                        .context("failed to send op response")?;
                }
                // Ignore empty responses
                None => {}
            };
        }

        bail!("Server unexpectedly closed connection")
    }

    #[instrument(level = "debug", skip(self), err)]
    async fn handle_op(
        &mut self,
        op: UnionResponse,
        // auth_context: &models::AuthContext,
        // call_depth should be mutable
        call_depth: u32,
    ) -> Result<ExecuteJsRequest> {
        trace!("handle_op: {:?}", op);
        self.state.ops_count += 1;

        // let action_ipfs_id = "".to_string(); // auth_context.action_ipfs_id_stack.last().cloned();

        Ok(match op {
            UnionResponse::SetResponse(SetResponseRequest { response }) => {
                if response.len() > self.max_response_length {
                    bail!(
                        "Response is too long. Max length is {} bytes",
                        self.max_response_length
                    );
                }
                self.state.response = response;
                SetResponseResponse {}.into()
            }
            UnionResponse::Print(PrintRequest { message }) => {
                if self.state.logs.len() + message.len() > self.max_console_log_length {
                    bail!(
                        "Console.log is printing something that is too long. Max length for all logs in a single request is {} bytes",
                        self.max_console_log_length
                    );
                }
                self.state.logs.push_str(&message);
                PrintResponse {}.into()
            }
            UnionResponse::IncrementFetchCount(IncrementFetchCountRequest {}) => {
                self.state.fetch_count += 1;
                // self.pay(LitActionPriceComponent::Fetches, 1).await?;
                if self.state.fetch_count > self.max_fetch_count {
                    bail!(
                        "You may not send more than {} HTTP requests per session and you have attempted to exceed that limit.",
                        self.max_fetch_count
                    );
                }
                IncrementFetchCountResponse {
                    fetch_count: self.state.fetch_count,
                }
                .into()
            }
            UnionResponse::PkpPermissionsGetPermitted(PkpPermissionsGetPermittedRequest {
                method: _,
                token_id: _,
                key_set_id: _,
            }) => {
                bail!("PkpPermissionsGetPermitted is not implemented");
            }
            UnionResponse::PkpPermissionsGetPermittedAuthMethodScopes(
                PkpPermissionsGetPermittedAuthMethodScopesRequest {
                    token_id: _,
                    method: _,
                    user_id: _,
                    max_scope_id: _,
                    key_set_id: _,
                },
            ) => {
                bail!("PkpPermissionsGetPermittedAuthMethodScopes is not implemented");
            }
            UnionResponse::PkpPermissionsIsPermitted(PkpPermissionsIsPermittedRequest {
                method: _,
                token_id: _,
                params: _,
                key_set_id: _,
            }) => {
                bail!("PkpPermissionsIsPermitted is not implemented");
            }
            UnionResponse::PkpPermissionsIsPermittedAuthMethod(
                PkpPermissionsIsPermittedAuthMethodRequest {
                    token_id: _,
                    method: _,
                    user_id: _,
                    key_set_id: _,
                },
            ) => {
                bail!("PkpPermissionsIsPermittedAuthMethod is not implemented");
            }
            UnionResponse::PubkeyToTokenId(PubkeyToTokenIdRequest {
                public_key: _,
                key_set_id: _,
            }) => {
                bail!("PubkeyToTokenId is not implemented");
            }
            UnionResponse::SignEcdsa(SignEcdsaRequest {
                to_sign ,
                public_key ,
                sig_name ,
                eth_personal_sign ,
                key_set_id ,
            }) => {
                let api_key = self.api_key.clone();
                let secret_u256 = crate::accounts::get_wallet_derivation(&api_key, &public_key).await?;
                let mut secret_bytes = [0; 32];
                secret_u256.to_big_endian(&mut secret_bytes);

                let signing_key = SigningKey::from_slice(&secret_bytes)?;

                let signature = signing_key.sign_recoverable(&to_sign)?;

                let hex_signature = bytes_to_hex(&signature.0.to_vec());

                self.state.sign_count += 1;
                
                // let recovery_id = signature.1.to_string();

                SignEcdsaResponse { success: hex_signature }.into()                
            }
            UnionResponse::Sign(SignRequest {
                to_sign: _,
                public_key: _,
                sig_name: _,
                signing_scheme: _,
                key_set_id: _,
            }) => {
                bail!("Sign is not implemented");
            }
            UnionResponse::AesDecrypt(AesDecryptRequest {
                symmetric_key: _,
                ciphertext: _,
            }) => {
                bail!("AesDecrypt is not implemented");
            }
            UnionResponse::GetLatestNonce(GetLatestNonceRequest {
                address: _,
                chain: _,
            }) => {
                bail!("GetLatestNonce is not implemented");
            }
            UnionResponse::CheckConditions(CheckConditionsRequest {
                conditions: _,
                auth_sig: _,
                chain: _,
            }) => {
                bail!("CheckConditions is not implemented");
            }
            UnionResponse::ClaimKeyIdentifier(ClaimKeyIdentifierRequest { key_id: _ }) => {
                bail!("ClaimKeyIdentifier is not implemented");
            }
            UnionResponse::CallContract(CallContractRequest { chain: _, txn: _ }) => {
                bail!("CallContract is not implemented");
            }
            UnionResponse::CallChild(CallChildRequest {
                ipfs_id: _,
                params: _,
            }) => {
                // self.pay(LitActionPriceComponent::CallDepth, 1).await?;

                // info!(
                //     "Calling child action: {:?}, self keyset id: {:?}",
                //     ipfs_id, self.key_set_id
                // );
                // call_depth += 1;
                // if call_depth > self.max_call_depth {
                //     bail!(
                //         "The recursion limit of a child action is {} and you have attempted to exceed that limit.",
                //         self.max_call_depth
                //     );
                // }

                // // Pull down the lit action code from IPFS
                // let code = crate::utils::web::get_ipfs_file(
                //     &ipfs_id,
                //     self.lit_config(),
                //     self.ipfs_cache()?,
                //     self.http_cache()?,
                // )
                // .await?;

                // let globals = params
                //     .map(|params| serde_json::from_slice::<serde_json::Value>(&params))
                //     .transpose()?;

                // let auth_context = {
                //     let mut ctx = auth_context.clone();
                //     ctx.action_ipfs_id_stack.push(ipfs_id.clone());
                //     ctx
                // };

                // // NB: Using execute_js_inner instead of execute_js to avoid resetting state
                // let res = Box::pin(self.execute_js_inner(code, globals, &auth_context, call_depth))
                //     .await?;

                // CallChildResponse {
                //     response: res.response,
                // }
                // .into()
                bail!("CallChild is not implemented");
            }
            UnionResponse::BroadcastAndCollect(BroadcastAndCollectRequest {
                name: _,
                value: _,
            }) => {
                // self.pay(LitActionPriceComponent::Broadcasts, 1).await?;

                // self.increment_broad_and_collect_counter()?;

                // let (tss_state, txn_prefix) = self.tss_state_and_txn_prefix()?;
                // let txn_prefix = format!("{txn_prefix}_{name}");

                // let tss_state = Arc::new(tss_state);
                // let cm = CommsManager::new(&tss_state, 0, &txn_prefix, "0", &self.node_set).await?;
                // let values = cm
                //     .broadcast_and_collect::<String, String>(value.clone())
                //     .await?;

                // let mut values: Vec<String> = values.into_iter().map(|(k, v)| v).collect();
                // values.push(value);

                // BroadcastAndCollectResponse { name, values }.into()
                bail!("BroadcastAndCollect is not implemented");
            }
            UnionResponse::DecryptAndCombine(DecryptAndCombineRequest {
                access_control_conditions: _,
                ciphertext: _,
                data_to_encrypt_hash: _,
                auth_sig: _,
                chain: _,
                key_set_id: _,
            }) => {
                bail!("DecryptAndCombine is not implemented");
            }
            UnionResponse::DecryptToSingleNode(DecryptToSingleNodeRequest {
                access_control_conditions: _,
                ciphertext: _,
                data_to_encrypt_hash: _,
                auth_sig: _,
                chain: _,
                key_set_id: _,
            }) => {
                bail!("DecryptToSingleNode is not implemented");
            }
            UnionResponse::SignAndCombineEcdsa(SignAndCombineEcdsaRequest {
                to_sign: _,
                public_key: _,
                sig_name: _,
                key_set_id: _,
            }) => {
                // we both the signatures and the broadcasts for
                bail!("SignAndCombineEcdsa is not implemented");
            }
            UnionResponse::SignAndCombine(SignAndCombineRequest {
                to_sign: _,
                public_key: _,
                sig_name: _,
                signing_scheme: _,
                key_set_id: _,
            }) => {
                bail!("SignAndCombine is not implemented");
            }
            UnionResponse::GetRpcUrl(GetRpcUrlRequest { chain: _ }) => {
                // let result = rpc_url(chain)
                //     .unwrap_or_else(|e| format!("Error getting RPC URL: {e:?}").to_string());
                // GetRpcUrlResponse { result }.into()
                bail!("GetRpcUrl is not implemented");
            }

            UnionResponse::P2pBroadcast(P2pBroadcastRequest { name: _, value: _ }) => {
                bail!("P2pBroadcast is not implemented");
            }

            UnionResponse::P2pCollectFromLeader(P2pCollectFromLeaderRequest { name: _ }) => {
                bail!("P2pCollectFromLeader is not implemented");
            }

            UnionResponse::IsLeader(IsLeaderRequest {}) => {
                bail!("IsLeader is not implemented");
            }

            UnionResponse::EncryptBls(EncryptBlsRequest {
                access_control_conditions: _,
                to_encrypt: _,
                key_set_id: _,
            }) => {
                bail!("EncryptBls is not implemented");
            }
            UnionResponse::UpdateResourceUsage(UpdateResourceUsageRequest {
                tick: _,
                used_kb: _,
            }) => {
                // // For now, we'll just return a success response
                // let r = self
                //     .dynamic_payment
                //     .add(LitActionPriceComponent::MemoryUsage, used_kb as u64);

                // let cancel_action = r.is_err();

                let cancel_action = false;

                UpdateResourceUsageResponse { cancel_action }.into()
            }
            UnionResponse::Result(_) => unreachable!(), // handled in main loop
            UnionResponse::SignAsAction(SignAsActionRequest {
                to_sign: _,
                sig_name: _,
                signing_scheme: _,
            }) => {
                bail!("SignAsAction is not implemented");
            }
            UnionResponse::GetActionPublicKey(GetActionPublicKeyRequest {
                signing_scheme: _,
                action_ipfs_cid: _,
            }) => {
                bail!("GetActionPublicKey is not implemented");
            }
            UnionResponse::VerifyActionSignature(VerifyActionSignatureRequest {
                signing_scheme: _,
                action_ipfs_cid: _,
                to_sign: _,
                sign_output: _,
            }) => {
                bail!("VerifyActionSignature is not implemented");
            }
        })
    }

    // async fn pay(&mut self, price_component: LitActionPriceComponent, price: u64) -> Result<()> {
    //     if let Err(e) = self.dynamic_payment.add(price_component, price) {
    //         bail!(e);
    //     }
    //     Ok(())
    // }

    fn increment_broad_and_collect_counter(&mut self) -> Result<()> {
        self.state.broadcast_and_collect_count += 1;
        if self.state.broadcast_and_collect_count > self.max_broadcast_and_collect_count {
            bail!(
                "You may not use broadcast and collect functionality more than {} times per session and you have attempted to exceed that limit.",
                self.max_broadcast_and_collect_count,
            );
        };
        Ok(())
    }
}
