use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use crate::cdn_module_loader::{CdnModuleLoader, ModuleCache};
use crate::runtime::{
    ActionCodeCache, DEFAULT_MEMORY_LIMIT_MB, PoolSharedState, new_action_code_cache,
};
use crate::v8_code_cache::{SharedV8CodeCache, new_v8_code_cache};
use crate::worker_pool::{WorkItem, WorkerPool};

use anyhow::Result;
use deno_core::error::CoreError;
use deno_core::futures::TryFutureExt as _;
use deno_lib::util::result::any_and_jserrorbox_downcast_ref;
use deno_runtime::tokio_util::create_and_run_current_thread;
use lit_actions_grpc::{proto::*, unix};
use lit_api_core::context::{HEADER_KEY_X_CORRELATION_ID, HEADER_KEY_X_REQUEST_ID};
use lit_observability::{
    PRIVACY_MODE_TAG,
    channels::{ChannelMsg, TracedReceiver, new_traced_bounded_channel},
    logging::{clear_task_request_context, set_request_context},
};
use temp_file::TempFile;
use tokio_stream::{Stream, StreamExt as _};
use tonic::{Request, Response, Status};
use tracing::{Instrument, Span, debug, debug_span, error, instrument};

/// Env var that overrides the default pool size. `0` disables the pool
/// entirely (every request goes via the legacy cold path). Default: 10.
const POOL_SIZE_ENV: &str = "LIT_ACTIONS_POOL_SIZE";
const DEFAULT_POOL_SIZE: usize = 10;

#[derive(Default, PartialEq)]
pub enum ServerType {
    #[default]
    Production,
    Test,
}

pub struct Server {
    server_type: ServerType,
    /// Parsed integrity manifest (URL → base64-encoded SHA-384 hash).
    integrity_manifest: Arc<RwLock<HashMap<String, String>>>,
    /// If true, reject modules not in the integrity manifest.
    strict_imports: bool,
    /// Shared cache for fetched CDN modules.
    module_cache: ModuleCache,
    /// Shared cache for action code prepared from the incoming code CID.
    action_code_cache: ActionCodeCache,
    /// Shared V8 code cache: handed to each MainWorker so compiled bytecode
    /// for cached actions is reused instead of being recompiled every run.
    v8_code_cache: SharedV8CodeCache,
    /// Path to integrity.lock file for TOFU auto-pinning.
    lockfile_path: Option<PathBuf>,
    /// Shared HTTP client for CDN fetches (connection pooling across executions).
    http_client: Arc<reqwest::Client>,
    /// Pre-warmed worker pool. `target_size = 0` disables the pool entirely.
    pool: Arc<WorkerPool>,
}

impl Server {
    fn into_service(self) -> ActionServer<Self> {
        ActionServer::new(self)
            // Let lit_node enforce size limits
            .max_decoding_message_size(usize::MAX)
    }

    pub fn pool(&self) -> Arc<WorkerPool> {
        self.pool.clone()
    }

    fn build_pool(
        integrity_manifest: Arc<RwLock<HashMap<String, String>>>,
        strict_imports: bool,
        module_cache: ModuleCache,
        lockfile_path: Option<PathBuf>,
        http_client: Arc<reqwest::Client>,
        v8_code_cache: SharedV8CodeCache,
    ) -> Arc<WorkerPool> {
        let shared = Arc::new(PoolSharedState {
            integrity_manifest,
            strict_imports,
            module_cache,
            lockfile_path,
            http_client,
            memory_limit_mb: DEFAULT_MEMORY_LIMIT_MB,
            v8_code_cache,
        });
        WorkerPool::new(pool_size_from_env(), shared)
    }

    /// Create a production server with an integrity manifest.
    pub fn with_integrity(
        integrity_manifest: Arc<RwLock<HashMap<String, String>>>,
        strict_imports: bool,
        lockfile_path: Option<PathBuf>,
    ) -> Self {
        let module_cache: ModuleCache = Arc::new(RwLock::new(HashMap::new()));
        let http_client = CdnModuleLoader::build_http_client();
        let v8_code_cache = new_v8_code_cache();
        let pool = Self::build_pool(
            integrity_manifest.clone(),
            strict_imports,
            module_cache.clone(),
            lockfile_path.clone(),
            http_client.clone(),
            v8_code_cache.clone(),
        );
        Self {
            server_type: ServerType::Production,
            integrity_manifest,
            strict_imports,
            module_cache,
            action_code_cache: new_action_code_cache(),
            v8_code_cache,
            lockfile_path,
            http_client,
            pool,
        }
    }

    pub fn new_test_server() -> Self {
        let module_cache: ModuleCache = Arc::new(RwLock::new(HashMap::new()));
        let http_client = CdnModuleLoader::build_http_client();
        let integrity_manifest = Arc::new(RwLock::new(HashMap::new()));
        let v8_code_cache = new_v8_code_cache();
        let pool = Self::build_pool(
            integrity_manifest.clone(),
            false,
            module_cache.clone(),
            None,
            http_client.clone(),
            v8_code_cache.clone(),
        );
        Self {
            server_type: ServerType::Test,
            integrity_manifest,
            strict_imports: false,
            module_cache,
            action_code_cache: new_action_code_cache(),
            v8_code_cache,
            lockfile_path: None,
            http_client,
            pool,
        }
    }
}

/// Per-request dispatch context cloned from `Server` once per incoming
/// request so the dispatcher can route between the pool and legacy paths
/// without holding a reference to `Server`.
#[derive(Clone)]
struct DispatchState {
    pool: Arc<WorkerPool>,
    is_test_server: bool,
    integrity_manifest: Arc<RwLock<HashMap<String, String>>>,
    strict_imports: bool,
    module_cache: ModuleCache,
    action_code_cache: ActionCodeCache,
    v8_code_cache: SharedV8CodeCache,
    lockfile_path: Option<PathBuf>,
    http_client: Arc<reqwest::Client>,
}

impl DispatchState {
    fn from_server(server: &Server) -> Self {
        Self {
            pool: server.pool.clone(),
            is_test_server: server.server_type == ServerType::Test,
            integrity_manifest: server.integrity_manifest.clone(),
            strict_imports: server.strict_imports,
            module_cache: server.module_cache.clone(),
            action_code_cache: server.action_code_cache.clone(),
            v8_code_cache: server.v8_code_cache.clone(),
            lockfile_path: server.lockfile_path.clone(),
            http_client: server.http_client.clone(),
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
        let dispatch = DispatchState::from_server(self);

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

                    dispatch_execute_request(&dispatch, req, outbound_tx, inbound_rx, span).await;
                }
                _ => {} // Ignore empty requests
            }
        });

        clear_task_request_context();
        Ok(Response::new(Box::pin(outbound_rx.into_stream())))
    }
}

/// Route an incoming `ExecuteJsRequest` to either the pre-warmed worker
/// pool or the legacy cold path.
///
/// Empty-code requests short-circuit BEFORE pool acquisition so they never
/// consume a pre-warmed worker.
///
/// Custom `memory_limit` requests bypass the pool entirely: pool workers
/// are bootstrapped at `DEFAULT_MEMORY_LIMIT_MB` and V8's heap limit is
/// immutable post-creation.
async fn dispatch_execute_request(
    dispatch: &DispatchState,
    req: ExecutionRequest,
    outbound_tx: flume::Sender<tonic::Result<ExecuteJsResponse>>,
    inbound_rx: TracedReceiver<ExecuteJsRequest>,
    span: Span,
) {
    // Empty-code shortcut: never consume a pre-warmed worker for this.
    // Mirrors the fast path in `runtime::execute_js` but moved up so the
    // pool isn't drained and refilled for a no-op.
    //
    // Use `send_async().await` rather than `try_send`: `outbound_tx` is a
    // rendezvous channel (`flume::bounded(0)`), and the receiver is the
    // tonic response stream. If the dispatcher fires before tonic begins
    // polling the stream, `try_send` drops the only response and the client
    // observes an empty stream / hang.
    if req.code.is_empty() || req.code.bytes().all(|b| b.is_ascii_whitespace()) {
        let _ = outbound_tx
            .send_async(Ok(ExecutionResult {
                success: true,
                ..Default::default()
            }
            .into()))
            .await;
        return;
    }

    let memory_limit_mb = req.memory_limit.map(|limit| limit as usize);
    // Custom-limit requests can't ride pool workers (heap limits are baked
    // in at isolate creation). Default-limit and unspecified-limit requests
    // can.
    let can_pool = match memory_limit_mb {
        None => true,
        Some(m) => m == DEFAULT_MEMORY_LIMIT_MB,
    };

    let request_id = req
        .http_headers
        .get(&HEADER_KEY_X_REQUEST_ID.to_ascii_lowercase())
        .cloned();
    let correlation_id = req
        .http_headers
        .get(&HEADER_KEY_X_CORRELATION_ID.to_ascii_lowercase())
        .cloned();

    let work = WorkItem {
        code: req.code,
        js_params: req.js_params.and_then(|v| serde_json::from_slice(&v).ok()),
        auth_context: req
            .auth_context
            .and_then(|v| serde_json::from_slice(&v).ok()),
        http_headers: req.http_headers,
        timeout_ms: req.timeout,
        outbound_tx: outbound_tx.clone(),
        inbound_rx,
        is_test_server: dispatch.is_test_server,
        action_code_cache: dispatch.action_code_cache.clone(),
        request_id,
        correlation_id,
        span,
    };

    if can_pool && let Some(handle) = dispatch.pool.try_acquire() {
        match handle.work_tx.try_send(work) {
            Ok(()) => return,
            Err(flume::TrySendError::Disconnected(reclaimed)) => {
                // Worker thread died between publishing its handle and
                // the dispatcher trying to hand it work. Reclaim the
                // WorkItem and route to legacy.
                dispatch.pool.note_disconnected();
                spawn_legacy(dispatch, reclaimed, memory_limit_mb);
                return;
            }
            Err(flume::TrySendError::Full(reclaimed)) => {
                // One-shot lifecycle invariant violation: the work
                // channel is bounded(1) and no work has been sent yet.
                // Logged at WARN with a separate counter; still safe.
                dispatch.pool.note_full_error();
                spawn_legacy(dispatch, reclaimed, memory_limit_mb);
                return;
            }
        }
        // Pool miss (empty ready channel, breaker open, etc): fall
        // through to the legacy cold path with the same WorkItem.
    }

    spawn_legacy(dispatch, work, memory_limit_mb);
}

/// Legacy cold path: spawn an OS thread, build a fresh `MainWorker` from
/// the snapshot inside `runtime::execute_js`, and execute. Preserves the
/// original execution model for custom `memory_limit` requests and as the
/// fallback when the pool can't service a request.
fn spawn_legacy(dispatch: &DispatchState, work: WorkItem, memory_limit_mb: Option<usize>) {
    let integrity_manifest = dispatch.integrity_manifest.clone();
    let strict_imports = dispatch.strict_imports;
    let module_cache = dispatch.module_cache.clone();
    let lockfile_path = dispatch.lockfile_path.clone();
    let http_client = dispatch.http_client.clone();
    let v8_code_cache = dispatch.v8_code_cache.clone();

    std::thread::spawn(move || {
        if work.request_id.is_some() || work.correlation_id.is_some() {
            set_request_context(work.request_id.clone(), work.correlation_id.clone());
        }

        let WorkItem {
            code,
            js_params,
            auth_context,
            http_headers,
            timeout_ms,
            outbound_tx,
            inbound_rx,
            is_test_server,
            action_code_cache,
            request_id: _,
            correlation_id: _,
            span,
        } = work;

        create_and_run_current_thread(
            async move {
                let res = crate::runtime::execute_js(
                    code,
                    js_params,
                    auth_context,
                    http_headers,
                    timeout_ms,
                    memory_limit_mb,
                    outbound_tx.clone(),
                    inbound_rx,
                    is_test_server,
                    integrity_manifest,
                    strict_imports,
                    module_cache,
                    action_code_cache,
                    v8_code_cache,
                    lockfile_path,
                    http_client,
                )
                .await;
                send_execution_result(&outbound_tx, res).await;
            }
            .instrument(span),
        );
    });
}

fn pool_size_from_env() -> usize {
    std::env::var(POOL_SIZE_ENV)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_POOL_SIZE)
}

pub(crate) fn format_error(err: &anyhow::Error) -> String {
    if let Some(CoreError::Js(js_err)) = any_and_jserrorbox_downcast_ref::<CoreError>(err) {
        deno_runtime::fmt_errors::format_js_error(js_err)
    } else {
        format!("{err:#}")
    }
}

/// Convert a runtime `Result<()>` to the response message expected by the
/// outbound stream and send it. Shared between the legacy cold path
/// (`server::spawn_legacy`) and the pool path
/// (`worker_pool::run_worker_thread`) so error formatting stays identical
/// across both.
pub(crate) async fn send_execution_result(
    outbound_tx: &flume::Sender<tonic::Result<ExecuteJsResponse>>,
    res: Result<()>,
) {
    let response = match res {
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
    };
    let _ = outbound_tx
        .send_async(response)
        .inspect_err(|e| error!("failed to send execution result: {e:#}"))
        .await;
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
    let server = Server::with_integrity(integrity_manifest, strict_imports, lockfile_path);
    // Spawn the warmup so it doesn't block socket bind. Worker threads do
    // the slow V8 / Deno bootstrap concurrently with the rest of startup;
    // the first few requests may miss the pool and fall back to legacy.
    let pool = server.pool();
    tokio::spawn(async move {
        pool.warmup();
    });
    unix::start_server(server.into_service(), socket_path, shutdown_signal).await
}

pub async fn start_test_server<P, S>(socket_path: P, shutdown_signal: Option<S>) -> Result<()>
where
    P: Into<PathBuf>,
    S: std::future::Future<Output = ()>,
{
    let server = Server::new_test_server();
    let pool = server.pool();
    tokio::spawn(async move {
        pool.warmup();
    });
    unix::start_server(server.into_service(), socket_path, shutdown_signal).await
}
pub struct TestServer {
    pub socket_file: TempFile,
    /// Snapshot of pool health counters; cloned at server construction
    /// so integration tests can assert on hits / misses without poking
    /// inside the running gRPC service.
    pub pool_health: Arc<crate::worker_pool::PoolHealth>,
    pub pool_target_size: usize,
}

impl TestServer {
    pub fn start() -> Self {
        let socket_file = temp_file::empty();
        let socket_path = socket_file.path().to_path_buf();

        // Build the server on the test thread so we can capture the pool
        // handle before moving the server into the runtime thread.
        let server = Server::new_test_server();
        let pool = server.pool();
        let pool_health = pool.health();
        let pool_target_size = pool.target_size();

        std::thread::spawn(move || {
            create_and_run_current_thread(async move {
                let signal = async {
                    let _ = tokio::signal::ctrl_c().await;
                };
                let warmup_pool = server.pool();
                tokio::spawn(async move {
                    warmup_pool.warmup();
                });
                unix::start_server(server.into_service(), socket_path, Some(signal))
                    .await
                    .expect("failed to start action server")
            });
        });

        // Wait for startup AND for the pool to warm enough for tests that
        // exercise warm hits. With pool=10 and a snapshot bootstrap of
        // ~30-50 ms per worker, 200 ms is enough for at least a few to be
        // ready. Tests that need every slot warm gate explicitly.
        std::thread::sleep(std::time::Duration::from_millis(200));

        Self {
            socket_file,
            pool_health,
            pool_target_size,
        }
    }

    pub fn socket_path(&self) -> PathBuf {
        self.socket_file.path().to_path_buf()
    }
}
