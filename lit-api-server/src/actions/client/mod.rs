#[allow(clippy::module_inception)]
pub mod client;
pub mod execution;
pub mod handle_ops;
pub mod models;
pub mod op_code_helpers;

use crate::actions::client::models::DenoExecutionEnv;
use crate::actions::client::models::ExecutionState;
use crate::actions::grpc::GrpcClientPool;
use derive_builder::Builder;
use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
const DEFAULT_MAX_NAMED_OUTPUT_COUNT: u32 = 100;
const DEFAULT_MAX_NAMED_OUTPUT_VALUE_LENGTH: usize = 1024; // 1KB
const DEFAULT_MAX_NAMED_OUTPUT_NAME_LENGTH: usize = 1024; // 1KB

#[derive(Debug, Default, Clone, Builder, Serialize, Deserialize)]
pub struct Client {
    #[builder(default, setter(into))]
    api_key: String,
    #[builder(default, setter(into))]
    ipfs_id: String,
    // Config
    #[builder(default, setter(into, strip_option))]
    socket_path: Option<PathBuf>,
    #[builder(default, setter(into))]
    #[serde(skip)]
    pub(crate) js_env: DenoExecutionEnv,
    #[builder(default, setter(into))]
    request_id: Option<String>,
    #[builder(default, setter(into))]
    http_headers: BTreeMap<String, String>,
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
    #[builder(default = "DEFAULT_MAX_NAMED_OUTPUT_COUNT")]
    max_named_output_count: u32,
    #[builder(default = "DEFAULT_MAX_NAMED_OUTPUT_VALUE_LENGTH")]
    max_named_output_value_length: usize,
    #[builder(default = "DEFAULT_MAX_NAMED_OUTPUT_NAME_LENGTH")]
    max_named_output_name_length: usize,
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
