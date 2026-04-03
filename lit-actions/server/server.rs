use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use crate::cdn_module_loader::ModuleCache;

use anyhow::Result;
use deno_core::error::CoreError;
use deno_core::futures::TryFutureExt as _;
use deno_lib::util::result::any_and_jserrorbox_downcast_ref;
use deno_runtime::tokio_util::create_and_run_current_thread;
use lit_actions_grpc::{proto::*, unix};
use lit_api_core::context::{HEADER_KEY_X_CORRELATION_ID, HEADER_KEY_X_REQUEST_ID};
use lit_observability::{
    PRIVACY_MODE_TAG,
    channels::{ChannelMsg, new_traced_bounded_channel},
    logging::{clear_task_request_context, set_request_context},
};
use temp_file::TempFile;
use tokio_stream::{Stream, StreamExt as _};
use tonic::{Request, Response, Status};
use tracing::{Instrument, debug, debug_span, error, instrument};

#[derive(Default, PartialEq)]
pub enum ServerType {
    #[default]
    Production,
    Test,
}

#[derive(Default)]
pub struct Server {
    server_type: ServerType,
    /// Parsed integrity manifest (URL → base64-encoded SHA-384 hash).
    integrity_manifest: Arc<RwLock<HashMap<String, String>>>,
    /// If true, reject modules not in the integrity manifest.
    strict_imports: bool,
    /// Shared cache for fetched CDN modules.
    module_cache: ModuleCache,
    /// Path to integrity.lock file for TOFU auto-pinning.
    lockfile_path: Option<PathBuf>,
}

impl Server {
    fn into_service(self) -> ActionServer<Self> {
        ActionServer::new(self)
            // Let lit_node enforce size limits
            .max_decoding_message_size(usize::MAX)
    }

    /// Create a production server with an integrity manifest.
    pub fn with_integrity(
        integrity_manifest: Arc<RwLock<HashMap<String, String>>>,
        strict_imports: bool,
        lockfile_path: Option<PathBuf>,
    ) -> Self {
        Self {
            server_type: ServerType::Production,
            integrity_manifest,
            strict_imports,
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            lockfile_path,
        }
    }

    fn new_test_server() -> Self {
        Self {
            server_type: ServerType::Test,
            ..Default::default()
        }
    }
}

#[tonic::async_trait]
impl Action for Server {
    type ExecuteJsStream =
        Pin<Box<dyn Stream<Item = Result<ExecuteJsResponse, Status>> + Send + 'static>>;

    #[allow(clippy::blocks_in_conditions)]
    #[instrument(skip_all, err)]
    async fn execute_js(
        &self,
        request: Request<tonic::Streaming<ExecuteJsRequest>>,
    ) -> Result<Response<Self::ExecuteJsStream>, Status> {
        let mut stream = request.into_inner();
        let (inbound_tx, inbound_rx) = new_traced_bounded_channel(0);
        let (outbound_tx, outbound_rx) = flume::bounded(0);
        let is_test_server = self.server_type == ServerType::Test;
        let integrity_manifest = self.integrity_manifest.clone();
        let strict_imports = self.strict_imports;
        let module_cache = self.module_cache.clone();
        let lockfile_path = self.lockfile_path.clone();

        // Put incoming requests into channel
        let send_exec_req_span = debug_span!("send_exec_req");
        tokio::spawn(
            async move {
                while let Ok(Some(req)) = stream.try_next().await {
                    let _ = inbound_tx
                        .send_async(req)
                        .inspect_err(|e| error!("failed to forward request: {e:#}"))
                        .await;
                }
            }
            .instrument(send_exec_req_span),
        );

        // Handle initial execution request, forward ops requests to the runtime
        tokio::spawn(async move {
            let (req, span) = match inbound_rx
                .recv_async()
                .inspect_err(|e| error!("failed to receive request: {e:#}"))
                .await
            {
                Ok(req) => req,
                Err(e) => {
                    error!("failed to receive request: {e:#}");
                    (
                        ChannelMsg::new(ExecuteJsRequest::default()),
                        debug_span!("recv_async"),
                    )
                }
            };
            let req = req.data().to_owned();

            let outbound_tx = outbound_tx.clone();
            let inbound_rx = inbound_rx.clone();

            #[allow(clippy::single_match)]
            match req.union {
                Some(UnionRequest::Execute(req)) => {
                    // Privacy mode is indicated by the PRIVACY_MODE_TAG suffix
                    // on the request_id, tagged at the origin (lit-api-server fairing).
                    let privacy_mode = req
                        .http_headers
                        .get(&HEADER_KEY_X_REQUEST_ID.to_ascii_lowercase())
                        .is_some_and(|id| id.ends_with(PRIVACY_MODE_TAG));

                    if privacy_mode {
                        debug!("ExecuteJsRequest: **PRIVACY MODE**");
                    } else {
                        debug!("{:?}", DebugExecutionRequest::from(&req));
                    }

                    std::thread::spawn(move || {
                        // Extract IDs from http_headers for log injection context.
                        // The request_id arrives pre-tagged with PRIVACY_MODE_TAG
                        // from lit-api-server if privacy mode was requested.
                        let request_id = req
                            .http_headers
                            .get(&HEADER_KEY_X_REQUEST_ID.to_ascii_lowercase())
                            .cloned();
                        let correlation_id = req
                            .http_headers
                            .get(&HEADER_KEY_X_CORRELATION_ID.to_ascii_lowercase())
                            .cloned();

                        if request_id.is_some() || correlation_id.is_some() {
                            set_request_context(request_id, correlation_id);
                        }

                        create_and_run_current_thread(
                            async move {
                                let res = crate::runtime::execute_js(
                                    req.code,
                                    req.js_params.and_then(|v| serde_json::from_slice(&v).ok()),
                                    req.auth_context
                                        .and_then(|v| serde_json::from_slice(&v).ok()),
                                    req.http_headers,
                                    req.timeout,
                                    req.memory_limit.map(|limit| limit as usize),
                                    outbound_tx.clone(),
                                    inbound_rx.clone(),
                                    is_test_server,
                                    integrity_manifest.clone(),
                                    strict_imports,
                                    module_cache.clone(),
                                    lockfile_path.clone(),
                                )
                                .await;
                                let _ = outbound_tx
                                    .send_async(match res {
                                        Ok(()) => Ok(ExecutionResult {
                                            success: true,
                                            ..Default::default()
                                        }
                                        .into()),
                                        Err(err) => {
                                            // Return Tonic error as-is, otherwise return ExecutionResult
                                            if let Some(status) = err.downcast_ref::<Status>() {
                                                error!("{status:#}");
                                                Err(status.clone())
                                            } else {
                                                Ok(ExecutionResult {
                                                    success: false,
                                                    error: format_error(&err),
                                                }
                                                .into())
                                            }
                                        }
                                    })
                                    .inspect_err(|e| {
                                        error!("failed to send execution result: {e:#}")
                                    })
                                    .await;
                            }
                            .instrument(span),
                        );
                    });
                }
                _ => {} // Ignore empty requests
            }
        });

        clear_task_request_context();
        Ok(Response::new(Box::pin(outbound_rx.into_stream())))
    }
}

fn format_error(err: &anyhow::Error) -> String {
    if let Some(CoreError::Js(js_err)) = any_and_jserrorbox_downcast_ref::<CoreError>(err) {
        deno_runtime::fmt_errors::format_js_error(js_err)
    } else {
        format!("{err:#}")
    }
}

pub async fn start_server<P, S>(
    socket_path: P,
    shutdown_signal: Option<S>,
    integrity_manifest: Arc<RwLock<HashMap<String, String>>>,
    strict_imports: bool,
    lockfile_path: Option<PathBuf>,
) -> Result<()>
where
    P: Into<PathBuf>,
    S: std::future::Future<Output = ()>,
{
    unix::start_server(
        Server::with_integrity(integrity_manifest, strict_imports, lockfile_path).into_service(),
        socket_path,
        shutdown_signal,
    )
    .await
}

pub async fn start_test_server<P, S>(socket_path: P, shutdown_signal: Option<S>) -> Result<()>
where
    P: Into<PathBuf>,
    S: std::future::Future<Output = ()>,
{
    unix::start_server(
        Server::new_test_server().into_service(),
        socket_path,
        shutdown_signal,
    )
    .await
}
pub struct TestServer {
    pub socket_file: TempFile,
}

impl TestServer {
    pub fn start() -> Self {
        let socket_file = temp_file::empty();
        let socket_path = socket_file.path().to_path_buf();

        std::thread::spawn(|| {
            create_and_run_current_thread(async move {
                let signal = async {
                    let _ = tokio::signal::ctrl_c().await;
                };
                start_test_server(socket_path, Some(signal))
                    .await
                    .expect("failed to start action server")
            });
        });

        // Wait for startup
        std::thread::sleep(std::time::Duration::from_millis(100));

        Self { socket_file }
    }

    pub fn socket_path(&self) -> PathBuf {
        self.socket_file.path().to_path_buf()
    }
}
