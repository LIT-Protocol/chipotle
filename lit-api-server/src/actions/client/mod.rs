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

pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 1000 * 60 * 15; // 15 minutes
pub(crate) const DEFAULT_ASYNC_TIMEOUT_MS: u64 = 1000 * 60 * 15; // 15 minutes
pub(crate) const DEFAULT_CLIENT_TIMEOUT_MS_BUFFER: u64 = 5_000;
pub(crate) const DEFAULT_MEMORY_LIMIT_MB: u32 = 64; // 64MB

pub(crate) const DEFAULT_MAX_CODE_LENGTH: usize = 16 * 1024 * 1024; // 16MB
pub(crate) const DEFAULT_MAX_CONSOLE_LOG_LENGTH: usize = 1024 * 100; // 100KB
pub(crate) const DEFAULT_MAX_FETCH_COUNT: u32 = 50;
pub(crate) const DEFAULT_MAX_RESPONSE_LENGTH: usize = 1024 * 100; // 100KB
pub(crate) const DEFAULT_MAX_GET_KEYS_COUNT: u32 = 10; // 10 signature requests per action execution
pub(crate) const DEFAULT_MAX_RETRIES: u32 = 3;

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
    #[builder(default = "DEFAULT_MAX_CONSOLE_LOG_LENGTH")]
    max_console_log_length: usize,
    #[builder(default = "DEFAULT_MAX_FETCH_COUNT")]
    max_fetch_count: u32,
    #[builder(default = "DEFAULT_MAX_GET_KEYS_COUNT")]
    max_get_keys_count: u32,
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
