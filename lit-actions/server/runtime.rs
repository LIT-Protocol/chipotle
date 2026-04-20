use std::collections::{BTreeMap, HashMap};
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Once;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use deno_core::{JsRuntime, v8};

use crate::bundler;
use crate::cdn_module_loader::{CdnModuleLoader, LoadedModules, ModuleCache};
use crate::import_rewriter;
use deno_resolver::npm::{DenoInNpmPackageChecker, ManagedNpmResolver};
use deno_runtime::{
    BootstrapOptions, WorkerLogLevel,
    deno_fs::RealFs,
    deno_permissions::{Permissions, PermissionsContainer, PermissionsOptions},
    fmt_errors::format_js_error,
    permissions::RuntimePermissionDescriptorParser,
    tokio_util::create_and_run_current_thread,
    worker::{MainWorker, WorkerOptions, WorkerServiceOptions},
};
use indoc::formatdoc;
use ipfs_hasher::IpfsHasher;
use lit_actions_grpc::proto::{ExecuteJsRequest, ExecuteJsResponse};
use lit_api_core::context::HEADER_KEY_X_PRIVACY_MODE;
use lit_observability::channels::TracedReceiver;
use lit_observability::logging::clear_task_request_context;
use sys_traits::impls::RealSys;
use tokio::sync::{mpsc, oneshot};
use tonic::Status;
use tracing::{debug, error, info_span, instrument};

// Same default limits as in lit-node's action client
const DEFAULT_TIMEOUT_MS: u64 = 1000 * 60 * 15; // 15 minutes
pub(crate) const DEFAULT_MEMORY_LIMIT_MB: usize = 128; // 128MB
const MEMORY_SAMPLE_INTERVAL_MS: u64 = 500; // 500ms
const EXECUTION_TERMINATED_ERROR: &str = "Uncaught Error: execution terminated";
const MAX_ACTION_CODE_CACHE_BYTES: usize = 100 * 1024 * 1024;
const ACTION_CODE_CACHE_TTL: Duration = Duration::from_secs(30 * 60);

pub(crate) type ActionCodeCache = Arc<RwLock<ActionCodeCacheState>>;

pub(crate) fn new_action_code_cache() -> ActionCodeCache {
    Arc::new(RwLock::new(ActionCodeCacheState::default()))
}

#[derive(Default)]
pub(crate) struct ActionCodeCacheState {
    entries: HashMap<String, ActionCodeCacheEntry>,
    total_bytes: usize,
}

struct ActionCodeCacheEntry {
    code: CachedActionCode,
    size_bytes: usize,
    expires_at: Instant,
}

struct ActionCodePrepareContext<'a> {
    client: &'a Arc<reqwest::Client>,
    integrity: &'a Arc<RwLock<HashMap<String, String>>>,
    strict_imports: bool,
    lockfile_path: &'a Option<PathBuf>,
    module_cache: &'a ModuleCache,
}

impl ActionCodeCacheState {
    fn get(&self, action_ipfs_id: &str, now: Instant) -> Option<CachedActionCode> {
        let entry = self.entries.get(action_ipfs_id)?;
        if entry.expires_at <= now {
            return None;
        }
        Some(entry.code.clone())
    }

    fn insert(&mut self, action_ipfs_id: String, code: CachedActionCode, now: Instant) -> bool {
        self.purge_expired(now);

        let size_bytes =
            action_code_cache_entry_size(&action_ipfs_id, action_ipfs_id.capacity(), &code);
        if size_bytes > MAX_ACTION_CODE_CACHE_BYTES {
            return false;
        }

        if let Some(old_entry) = self.entries.remove(&action_ipfs_id) {
            self.total_bytes = self.total_bytes.saturating_sub(old_entry.size_bytes);
        }

        if self.total_bytes + size_bytes > MAX_ACTION_CODE_CACHE_BYTES {
            return false;
        }

        self.total_bytes += size_bytes;
        self.entries.insert(
            action_ipfs_id,
            ActionCodeCacheEntry {
                code,
                size_bytes,
                expires_at: now + ACTION_CODE_CACHE_TTL,
            },
        );
        true
    }

    fn purge_expired(&mut self, now: Instant) {
        let expired_keys: Vec<String> = self
            .entries
            .iter()
            .filter_map(|(key, entry)| {
                if entry.expires_at <= now {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in expired_keys {
            if let Some(entry) = self.entries.remove(&key) {
                self.total_bytes = self.total_bytes.saturating_sub(entry.size_bytes);
            }
        }
    }

    fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

fn action_code_cache_entry_size(
    action_ipfs_id: &str,
    action_ipfs_id_capacity: usize,
    code: &CachedActionCode,
) -> usize {
    size_of::<ActionCodeCacheEntry>()
        + size_of::<String>()
        + action_ipfs_id_capacity.max(action_ipfs_id.len())
        + code.allocated_bytes()
}

#[derive(Clone, Debug)]
pub(crate) struct CachedActionCode {
    /// Fully self-contained JavaScript. When the user had static imports, this
    /// is the `swc_bundler` output with every transitive CDN dependency inlined
    /// and static ESM `import` / `export` syntax removed. User-authored dynamic
    /// `import()` expressions are not transformed by bundling, are unsupported,
    /// and may still appear in untouched user code (failing later at runtime).
    /// Params are wrapped around this at execute-time (never cached).
    code: String,
    /// URL→hash of every module the bundler fetched while producing `code`.
    /// Replayed into the per-execution `LoadedModules` tracker on cache hit so
    /// billing / `showImportDetails()` sees the same set as on a cache miss.
    loaded_modules: Vec<(String, String)>,
}

impl CachedActionCode {
    fn allocated_bytes(&self) -> usize {
        self.code.capacity()
            + self.loaded_modules.capacity() * size_of::<(String, String)>()
            + self
                .loaded_modules
                .iter()
                .map(|(url, hash)| url.capacity() + hash.capacity())
                .sum::<usize>()
    }

    fn to_executable_code(&self, js_func_params: &str) -> String {
        format!(
            "
        {code}
        ;
        (async () => {{
        const params = {js_func_params} ;
        const data = await main(params);
        if (typeof data !== \"undefined\") {{
          LitActions.setResponse( {{ response: data }} );
        }}
        }})();",
            code = self.code,
        )
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum ExecutionResult {
    Complete,
    Timeout,
    OutOfMemory,
}

/// State shared across all worker constructions in a single process.
/// Built once at server startup; cloned cheaply via `Arc` clone of fields.
#[derive(Clone)]
pub(crate) struct PoolSharedState {
    pub integrity_manifest: Arc<RwLock<HashMap<String, String>>>,
    pub strict_imports: bool,
    pub module_cache: ModuleCache,
    pub lockfile_path: Option<PathBuf>,
    pub http_client: Arc<reqwest::Client>,
    /// Heap limit in MB. Pool workers always use `DEFAULT_MEMORY_LIMIT_MB`;
    /// the legacy cold path uses whatever the caller specified.
    pub memory_limit_mb: usize,
}

/// A `MainWorker` that has been bootstrapped from the V8 snapshot but has
/// not yet had any per-request JS injected. Carries the `LoadedModules`
/// handle that was wired into its `CdnModuleLoader`; the same handle is
/// later inserted into `op_state` so billing sees a single source of truth.
///
/// `PreparedWorker` is consumed by value in `execute_with_worker` to enforce
/// one-shot lifecycle at the type level: the worker cannot be reused after
/// a single execution.
pub(crate) struct PreparedWorker {
    pub worker: MainWorker,
    pub loaded_modules: LoadedModules,
}

#[cfg(test)]
impl PreparedWorker {
    pub(crate) fn loaded_modules_for_test(&self) -> LoadedModules {
        self.loaded_modules.clone()
    }
}

static RUNTIME_SNAPSHOT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/RUNTIME_SNAPSHOT.bin"));

fn deno_isolate_init() -> Option<&'static [u8]> {
    debug!("Deno isolate init with snapshots.");
    Some(RUNTIME_SNAPSHOT)
}

fn get_lit_action_ipfs_id(code: &str) -> String {
    let ipfs_hasher = IpfsHasher::default();
    ipfs_hasher.compute(code.as_bytes())
}

fn record_loaded_modules(loaded_modules: &LoadedModules, cached_code: &CachedActionCode) {
    if let Ok(mut modules) = loaded_modules.0.write() {
        for (url, hash) in &cached_code.loaded_modules {
            if !modules.iter().any(|m| &m.url == url) {
                modules.push(crate::cdn_module_loader::LoadedModuleInfo {
                    url: url.clone(),
                    hash: hash.clone(),
                });
            }
        }
    }
}

async fn prepare_action_code(
    code: &str,
    context: &ActionCodePrepareContext<'_>,
) -> Result<CachedActionCode> {
    // Detect the user's static imports. If there are none, the raw code is
    // already executable as a script. Otherwise, hand off to the pre-execution
    // bundler, which inlines every CDN dep into the entry source so the cached
    // form contains no `import` call of any kind — eliminating Deno's
    // `resolve`/`load` pipeline from the hot path (CPL-262).
    //
    // `rewrite_imports` is used only to decide whether bundling is necessary
    // and to seed the dep-graph walk; the bundler consumes the **original**
    // code (with imports intact) so SWC can map each imported binding to its
    // inlined definition.
    let import_rewriter::RewriteResult { code: _, imports } =
        import_rewriter::rewrite_imports(code);

    if imports.is_empty() {
        return Ok(CachedActionCode {
            code: code.to_string(),
            loaded_modules: Vec::new(),
        });
    }

    let bundle_loader = CdnModuleLoader::with_options(
        context.integrity.clone(),
        context.strict_imports,
        context.module_cache.clone(),
        context.lockfile_path.clone(),
        Some(context.client.clone()),
        LoadedModules::default(),
    );
    let bundled = bundler::bundle_user_code(code, &imports, &bundle_loader)
        .await
        .map_err(|e| anyhow!("Failed to bundle CDN imports: {e}"))?;

    // Snapshot the modules the bundler fetched, so cache hits can replay
    // them into the per-execution LoadedModules handle for billing.
    let loaded_modules = bundle_loader
        .loaded_modules()
        .0
        .read()
        .map(|m| {
            m.iter()
                .map(|info| (info.url.clone(), info.hash.clone()))
                .collect()
        })
        .unwrap_or_default();

    Ok(CachedActionCode {
        code: bundled,
        loaded_modules,
    })
}

async fn get_or_prepare_action_code(
    code: &str,
    action_ipfs_id: &str,
    action_code_cache: &ActionCodeCache,
    prepare_context: &ActionCodePrepareContext<'_>,
) -> Result<CachedActionCode> {
    let now = Instant::now();
    if let Ok(cache) = action_code_cache.read()
        && let Some(cached_code) = cache.get(action_ipfs_id, now)
    {
        debug!(action_ipfs_id, "action code loaded from cache");
        return Ok(cached_code.clone());
    }

    let prepared_code = prepare_action_code(code, prepare_context).await?;

    if let Ok(mut cache) = action_code_cache.write() {
        if let Some(cached_code) = cache.get(action_ipfs_id, now) {
            return Ok(cached_code.clone());
        }

        if cache.insert(action_ipfs_id.to_string(), prepared_code.clone(), now) {
            debug!(
                action_ipfs_id,
                allocated_bytes = prepared_code.allocated_bytes(),
                ttl_seconds = ACTION_CODE_CACHE_TTL.as_secs(),
                "action code cached for future requests",
            );
        } else {
            debug!(
                action_ipfs_id,
                cache_bytes = cache.total_bytes(),
                allocated_bytes = prepared_code.allocated_bytes(),
                max_cache = MAX_ACTION_CODE_CACHE_BYTES,
                "action code cache full, skipping cache insertion"
            );
        }
    }

    Ok(prepared_code)
}

/// Build a `MainWorker` from the V8 snapshot. Warm-time safe: does no JS
/// injection, so it can run off the request path before `auth_context` and
/// `http_headers` are known. The single `LoadedModules` handle wired into
/// the `CdnModuleLoader` is also returned on the `PreparedWorker`, so the
/// caller can insert the same `Arc` into `op_state` later.
#[instrument(skip_all, err)]
pub(crate) fn build_worker_base(shared: &PoolSharedState) -> Result<PreparedWorker> {
    let loaded_modules = LoadedModules::default();
    let module_loader: Rc<dyn deno_core::ModuleLoader> = Rc::new(CdnModuleLoader::with_options(
        shared.integrity_manifest.clone(),
        shared.strict_imports,
        shared.module_cache.clone(),
        shared.lockfile_path.clone(),
        Some(shared.http_client.clone()),
        loaded_modules.clone(),
    ));

    let options = WorkerOptions {
        bootstrap: BootstrapOptions {
            cpu_count: 1,
            log_level: WorkerLogLevel::Info,
            no_color: true,
            user_agent: "lit_protocol_node".to_string(),
            ..Default::default()
        },
        extensions: vec![lit_actions_ext::lit_actions::init_ops()],
        startup_snapshot: deno_isolate_init(),
        skip_op_registration: false,
        create_params: Some(
            v8::CreateParams::default().heap_limits(0, shared.memory_limit_mb * 1024 * 1024),
        ),
        unsafely_ignore_certificate_errors: None,
        seed: None,
        create_web_worker_cb: Arc::new(|_| {
            unreachable!("web workers are disabled in PatchDeno.js")
        }),
        format_js_error_fn: Some(Arc::new(format_js_error)),
        maybe_inspector_server: None,
        should_break_on_first_statement: false,
        should_wait_for_inspector_session: false,
        strace_ops: None,
        cache_storage_dir: None,
        origin_storage_dir: None,
        stdio: Default::default(),
        enable_stack_trace_arg_in_ops: false,
    };

    // Deny everything except for network access, e.g. via fetch()
    let desc_parser = Arc::new(RuntimePermissionDescriptorParser::new(RealSys));
    let perms = Permissions::from_options(
        desc_parser.as_ref(),
        &PermissionsOptions {
            allow_net: Some(vec![]),
            ..Default::default()
        },
    )
    .expect("valid permissions");

    let services =
        WorkerServiceOptions::<DenoInNpmPackageChecker, ManagedNpmResolver<RealSys>, RealSys> {
            blob_store: Default::default(),
            broadcast_channel: Default::default(),
            feature_checker: Default::default(),
            fs: Arc::new(RealFs),
            module_loader,
            node_services: Default::default(),
            npm_process_state_provider: Default::default(),
            permissions: PermissionsContainer::new(desc_parser, perms),
            root_cert_store_provider: Default::default(),
            fetch_dns_resolver: Default::default(),
            shared_array_buffer_store: Default::default(),
            compiled_wasm_module_store: Default::default(),
            v8_code_cache: Default::default(),
        };

    let main_module =
        deno_core::resolve_url_or_path("./$lit$actions.js", Path::new(env!("CARGO_MANIFEST_DIR")))?;
    let worker = MainWorker::bootstrap_from_options(&main_module, services, options);

    Ok(PreparedWorker {
        worker,
        loaded_modules,
    })
}

/// Inject `LitNamespace.js` (LitActions / LitAuth / LitHeaders globals).
/// Request-time only: needs `auth_context` and `http_headers`. Runs BEFORE
/// `PatchDeno.js` so it can rely on the unmodified `Deno` namespace if needed.
pub(crate) fn inject_lit_namespace(
    worker: &mut MainWorker,
    auth_context: &Option<serde_json::Value>,
    http_headers: &BTreeMap<String, String>,
) -> Result<()> {
    let _span = info_span!("LitNamespace.js").entered();

    if http_headers
        .get(&HEADER_KEY_X_PRIVACY_MODE.to_ascii_lowercase())
        .is_some_and(|v| v == "true")
    {
        debug!("Populating LitHeaders: **PRIVACY MODE**");
    } else {
        debug!("Populating LitHeaders: {http_headers:?}");
    }

    // NB: globalThis.LitActions is already part of the V8 snapshot
    let mut code = formatdoc! {r#"
        "use strict";
        (function (actions, auth, headers) {{
            const {{ freeze }} = Object;
            const readOnly = value => ({{ value, enumerable: true, writable: false, configurable: false }});

            Object.defineProperties(globalThis, {{
                LitActions: readOnly(freeze(actions)),
                LitAuth: readOnly(freeze(auth)),
                LitHeaders: readOnly(headers),
            }});

            Object.defineProperty(globalThis, "Lit", readOnly(freeze({{
                Actions: LitActions,
                Auth: LitAuth,
                Headers: LitHeaders,
            }})));
        }})(LitActions, {auth}, new Headers({headers}));
        "#,
        auth = if let Some(ctx) = auth_context {
            serde_json::to_string(ctx).context("Could not serialize auth_context")?
        } else {
            "{}".to_string()
        },
        headers = serde_json::to_string(http_headers).context("Could not serialize HTTP headers")?
    };

    // Remove LitTest from non-debug builds
    if !cfg!(debug_assertions) {
        code.push_str("delete globalThis.LitTest;\n");
    }

    worker
        .execute_script("LitNamespace.js", code.into())
        .context("Error populating Lit namespace")?;

    Ok(())
}

/// Strip privileged `Deno.*` surface and disable `Worker`. Must run AFTER
/// `inject_lit_namespace` to preserve today's bootstrap order.
fn execute_patch_deno(worker: &mut MainWorker) -> Result<()> {
    let _span = info_span!("PatchDeno.js").entered();

    let code = formatdoc! {r#"
        "use strict";
        delete Deno.build;
        delete Deno.permissions;
        delete Deno.version;
        delete globalThis.Worker;
    "#};

    worker
        .execute_script("PatchDeno.js", code.into())
        .context("Error patching Deno runtime")?;

    Ok(())
}

/// Inject caller-supplied globals via `Params.js`. Reserved for future use:
/// `execute_js` always passes `None` today, so this is normally a no-op.
fn inject_params_globals(
    worker: &mut MainWorker,
    globals_to_inject: &Option<serde_json::Value>,
    http_headers: &BTreeMap<String, String>,
) -> Result<()> {
    let Some(params) = globals_to_inject else {
        return Ok(());
    };

    let _span = info_span!("Params.js").entered();

    if http_headers
        .get(&HEADER_KEY_X_PRIVACY_MODE.to_ascii_lowercase())
        .is_some_and(|v| v == "true")
    {
        debug!("Injecting params as globals: **PRIVACY MODE**");
    } else {
        debug!("Injecting params as globals: {params:?}");
    }

    let _ = params
        .as_object()
        .context("Could not convert params to map")?;

    let code = formatdoc! {r#"
        "use strict";
        Object.assign(globalThis, {params});
    "#};

    worker
        .execute_script("Params.js", code.into())
        .context("Error injecting params as globals")?;

    Ok(())
}

// NB: Due to the new PKU feature introduced in V8 11.6, we need to init the V8
// platform on the parent thread that will spawn V8 isolates (in main.rs).
// See https://github.com/denoland/deno/blob/v1.43/cli/main.rs
#[instrument(skip_all)]
pub fn init_v8() {
    // Tigthen up V8 security while sacrificing performance
    // To get a list of supported flags: deno run --v8-flags=-help
    let unknown_flags = &deno_core::v8_set_flags(vec![
        "UNUSED_BUT_NECESSARY_ARG0".into(), // See https://github.com/denoland/deno/blob/v1.37/cli/util/v8.rs#L17
        "--disallow-code-generation-from-strings".into(), // Disallow eval and friends
        "--memory-protection-keys".into(),  // Protect code memory with PKU if available
        "--clear-free-memory".into(),       // Initialize free memory with 0
    ])[1..];
    assert_eq!(
        unknown_flags,
        Vec::<String>::new(),
        "unknown V8 flags specified"
    );

    JsRuntime::init_platform(None, false);
}

/// Legacy cold path: build a one-off `PoolSharedState` with the caller's
/// `memory_limit_mb`, bootstrap a fresh `MainWorker`, inject per-request
/// state, and execute.
///
/// Pool path callers (see `worker_pool::WorkerPool`) instead call
/// `build_worker_base` once at warm-time, then `inject_lit_namespace` +
/// `execute_with_worker` per request against a pre-warmed `PreparedWorker`.
#[allow(clippy::too_many_arguments)]
#[instrument(skip_all, err)]
pub(crate) async fn execute_js(
    code: String,
    js_params: Option<serde_json::Value>,
    auth_context: Option<serde_json::Value>,
    http_headers: BTreeMap<String, String>,
    timeout_ms: Option<u64>,
    memory_limit_mb: Option<usize>,
    outbound_tx: flume::Sender<tonic::Result<ExecuteJsResponse>>,
    inbound_rx: TracedReceiver<ExecuteJsRequest>,
    is_test_server: bool,
    integrity_manifest: Arc<RwLock<HashMap<String, String>>>,
    strict_imports: bool,
    module_cache: ModuleCache,
    action_code_cache: ActionCodeCache,
    lockfile_path: Option<PathBuf>,
    http_client: Arc<reqwest::Client>,
) -> Result<()> {
    // Fast path to do nothing, allowing us to benchmark with and without Deno involved
    if code.is_empty() || code.bytes().all(|b| b.is_ascii_whitespace()) {
        return Ok(());
    }

    // Don't output ANSI escape sequences, e.g. when formatting JS errors
    static COLOR_INIT: Once = Once::new();
    COLOR_INIT.call_once(|| deno_runtime::colors::set_use_color(false));

    let shared = Arc::new(PoolSharedState {
        integrity_manifest,
        strict_imports,
        module_cache,
        lockfile_path,
        http_client,
        memory_limit_mb: memory_limit_mb.unwrap_or(DEFAULT_MEMORY_LIMIT_MB),
    });

    let mut prepared = build_worker_base(&shared)
        .context("Error building main worker")
        .map_err(|e| anyhow!("{e:#}"))?; // Ensure to keep context when downcasting JS errors later

    inject_lit_namespace(&mut prepared.worker, &auth_context, &http_headers)?;

    execute_with_worker(
        prepared,
        shared,
        code,
        js_params,
        http_headers,
        timeout_ms,
        outbound_tx,
        inbound_rx,
        is_test_server,
        action_code_cache,
    )
    .await
}

/// Run a single request against an already-bootstrapped `PreparedWorker`.
/// Consumes `prepared` by value to enforce one-shot lifecycle: the worker
/// is dropped when this function returns. Owns everything from the post-
/// bootstrap body of the original `execute_js`: `PatchDeno.js`, op-state
/// wiring, controller thread, near-heap callback, resource-usage polling,
/// action code cache lookup, user code execution, termination select loop,
/// and final `clear_task_request_context`.
#[allow(clippy::too_many_arguments)]
#[instrument(skip_all, err)]
pub(crate) async fn execute_with_worker(
    prepared: PreparedWorker,
    shared: Arc<PoolSharedState>,
    code: String,
    js_params: Option<serde_json::Value>,
    http_headers: BTreeMap<String, String>,
    timeout_ms: Option<u64>,
    outbound_tx: flume::Sender<tonic::Result<ExecuteJsResponse>>,
    inbound_rx: TracedReceiver<ExecuteJsRequest>,
    is_test_server: bool,
    action_code_cache: ActionCodeCache,
) -> Result<()> {
    let result = execute_with_worker_inner(
        prepared,
        shared,
        code,
        js_params,
        http_headers,
        timeout_ms,
        outbound_tx,
        inbound_rx,
        is_test_server,
        action_code_cache,
    )
    .await;

    // Always clear request context at the end of a request. The pool
    // worker thread is reused for refill state machinery before being
    // dropped; the legacy thread is also reused by the OS thread pool
    // implicitly. Either way, leaving stale tracing fields would leak
    // request_id / correlation_id into unrelated spans.
    clear_task_request_context();

    result
}

#[allow(clippy::too_many_arguments)]
async fn execute_with_worker_inner(
    prepared: PreparedWorker,
    shared: Arc<PoolSharedState>,
    code: String,
    js_params: Option<serde_json::Value>,
    http_headers: BTreeMap<String, String>,
    timeout_ms: Option<u64>,
    outbound_tx: flume::Sender<tonic::Result<ExecuteJsResponse>>,
    inbound_rx: TracedReceiver<ExecuteJsRequest>,
    is_test_server: bool,
    action_code_cache: ActionCodeCache,
) -> Result<()> {
    let PreparedWorker {
        mut worker,
        loaded_modules,
    } = prepared;

    let timeout_ms = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let memory_limit_mb = shared.memory_limit_mb;

    // PatchDeno.js must run AFTER LitNamespace.js (which the caller already
    // injected) to preserve the original bootstrap order.
    execute_patch_deno(&mut worker)?;
    // Reserved hook: today's call sites always pass globals=None; preserved
    // here so re-enabling globals injection drops in at the historical spot
    // (between PatchDeno.js and op-state wiring).
    inject_params_globals(&mut worker, &None, &http_headers)?;

    // Check the action code cache early so we can skip prepare_action_code
    // on cache hit (the real performance win). We always use CdnModuleLoader
    // regardless of cache hit/miss so that runtime dynamic imports (e.g.
    // `await import("zod@3.22.4")`) continue to work even for cached actions.
    let action_ipfs_id = get_lit_action_ipfs_id(&code);
    let now = Instant::now();
    let cached_code = action_code_cache
        .read()
        .ok()
        .and_then(|cache| cache.get(&action_ipfs_id, now));

    if cached_code.is_some() {
        debug!(action_ipfs_id, "action code cache hit");
    }

    let op_state = worker.js_runtime.op_state();
    {
        // scope the borrow
        let mut state = op_state.borrow_mut();
        state.put(outbound_tx);
        state.put(inbound_rx);
        state.put(loaded_modules.clone());
        drop(state);
    }

    let (halt_isolate_tx, mut halt_isolate_rx) = oneshot::channel::<ExecutionResult>();
    let (memory_limit_tx, memory_limit_rx) = mpsc::unbounded_channel::<usize>();

    start_controller_thread(
        &mut worker.js_runtime,
        timeout_ms,
        memory_limit_rx,
        halt_isolate_tx,
    );

    // Terminate isolate when approaching memory limit
    worker
        .js_runtime
        .add_near_heap_limit_callback(move |current_limit, _initial_limit| {
            let _ = memory_limit_tx.send(current_limit);
            current_limit * 2
        });

    let mut interval = tokio::time::interval(Duration::from_millis(MEMORY_SAMPLE_INTERVAL_MS));
    let mut heap_stats = v8::HeapStatistics::default();
    // we try to take a sample prior to event starting execution to get a baseline to charge - if the action is fast enough, we'll never get a tick !
    update_resource_usage(
        &mut worker.js_runtime,
        &mut heap_stats,
        tokio::time::Instant::now(),
        is_test_server,
    )
    .await?;

    let js_params = js_params.as_ref().unwrap_or_default();
    let js_func_params = js_params.to_string();

    // Params are injected per-execution and MUST NOT be part of the cached
    // form: they change between invocations while the bundled JS stays the
    // same. On cache hit we reuse the prepared form; on miss we prepare it
    // (bundle CDN deps, snapshot loaded modules, cache the result).
    let cached_code = if let Some(cached) = cached_code {
        cached
    } else {
        let prepare_context = ActionCodePrepareContext {
            client: &shared.http_client,
            integrity: &shared.integrity_manifest,
            strict_imports: shared.strict_imports,
            lockfile_path: &shared.lockfile_path,
            module_cache: &shared.module_cache,
        };
        get_or_prepare_action_code(&code, &action_ipfs_id, &action_code_cache, &prepare_context)
            .await?
    };
    record_loaded_modules(&loaded_modules, &cached_code);
    let code = cached_code.to_executable_code(&js_func_params);

    if let Err(e) = worker
        .js_runtime
        .execute_script("<user_provided_script>", code)
    {
        // Delay error handling if the controller has terminated the isolate,
        // in which case halt_isolate_rx will tell the reason.
        if e.to_string() != EXECUTION_TERMINATED_ERROR {
            bail!(e);
        }
    };

    loop {
        tokio::select! {
            biased;

            execution_result = &mut halt_isolate_rx => {
                break match execution_result {
                    Ok(ExecutionResult::Complete) => Ok(()),
                    Ok(ExecutionResult::Timeout) => bail!(Status::deadline_exceeded(format!("Your function exceeded the maximum runtime of {timeout_ms}ms and was terminated."))),
                    Ok(ExecutionResult::OutOfMemory) => bail!(Status::resource_exhausted(format!("Your function exceeded the maximum memory of {memory_limit_mb} MB and was terminated."))),
                    Err(e) => Err(e),
                }
            }

            event_loop_result = worker.run_event_loop(false) => {
                match event_loop_result {
                    Ok(()) => break Ok(()),
                    // Despite biased polling, the event loop may still complete first,
                    // e.g. when a promise resolves after 3s and the timeout is also 3s.
                    Err(e) if e.to_string() != EXECUTION_TERMINATED_ERROR => bail!(e),
                    Err(_) => {} // continue loop to wait for halt_isolate_rx
                }
            }
            tick_no = interval.tick() => {
                // note that if we error out trying to update resource usage, we will not continue execution
                update_resource_usage(&mut worker.js_runtime, &mut heap_stats, tick_no, is_test_server).await?;
            }
        }
    }?;

    Ok(())
}

#[instrument(skip_all, err)]
async fn update_resource_usage(
    js_runtime: &mut JsRuntime,
    heap_stats: &mut v8::HeapStatistics,
    tick_no: tokio::time::Instant,
    is_test_server: bool,
) -> Result<()> {
    if is_test_server {
        debug!("Skipping resource usage update in test server");
        return Ok(());
    }

    let op_state = js_runtime.op_state();
    js_runtime.v8_isolate().get_heap_statistics(heap_stats);
    let mb_used_heap_size = heap_stats.used_heap_size() / 1024 / 1024;
    debug!(
        "MB used at {}: {}",
        tick_no.elapsed().as_millis(),
        mb_used_heap_size
    );
    match lit_actions_ext::bindings::op_update_resource_usage_external(
        op_state,
        tick_no.elapsed().as_millis() as u32,
        mb_used_heap_size as u32,
    )
    .await
    {
        Ok(cancel_execution) => {
            if !cancel_execution {
                debug!("UpdateResourceUsage: {:?}", tick_no.elapsed().as_millis());
            } else {
                bail!(Status::resource_exhausted(
                    "Your function ran out of funds to continue execution and was terminated."
                        .to_string()
                ))
            }
        }
        Err(e) => {
            error!(
                "Error communicating with the lit-node to update resource usage: {:?}",
                e
            );

            bail!(Status::resource_exhausted(
                "Error communicating with the lit-node to update resource usage.".to_string()
            ))
        }
    }
    Ok(())
}

#[instrument(skip_all)]
fn start_controller_thread(
    js_runtime: &mut JsRuntime,
    worker_timeout_ms: u64,
    mut memory_limit_rx: mpsc::UnboundedReceiver<usize>,
    halt_isolate_tx: oneshot::Sender<ExecutionResult>,
) -> thread::JoinHandle<ExecutionResult> {
    let isolate_handle = js_runtime.v8_isolate().thread_safe_handle();

    thread::spawn(move || {
        let res = create_and_run_current_thread(async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(worker_timeout_ms)) => {
                    if isolate_handle.terminate_execution() {
                        debug!("Terminated isolate after timeout of {worker_timeout_ms}ms");
                        ExecutionResult::Timeout
                    } else {
                        // Isolate already terminated
                        ExecutionResult::Complete
                    }
                }
                limit = memory_limit_rx.recv() => {
                    if let Some(limit) = limit {
                        if isolate_handle.terminate_execution() {
                            debug!("Terminated isolate after reaching memory limit of {limit} bytes");
                            ExecutionResult::OutOfMemory
                        } else {
                            // Isolate already terminated
                            ExecutionResult::Complete
                        }
                    } else {
                        // Sender dropped before exceeding limits, stop controller thread
                        ExecutionResult::Complete
                    }
                }
            }
        });

        // Ignore error of receiver being dropped
        let _ = halt_isolate_tx.send(res);

        debug!("start_controller_thread: {res:?}");

        res
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached_code(code: &str) -> CachedActionCode {
        CachedActionCode {
            code: code.to_string(),
            loaded_modules: Vec::new(),
        }
    }

    #[test]
    fn action_code_cache_entry_size_counts_entry_overhead() {
        let action_ipfs_id = "QmTest".to_string();
        let code = cached_code("x");
        let size = action_code_cache_entry_size(&action_ipfs_id, action_ipfs_id.capacity(), &code);

        assert!(size > code.allocated_bytes() + action_ipfs_id.capacity());
    }

    #[test]
    fn action_code_cache_entry_size_uses_allocated_capacity() {
        let mut action_ipfs_id = String::with_capacity(64);
        action_ipfs_id.push_str("QmTest");

        let mut code_string = String::with_capacity(128);
        code_string.push('x');

        let mut loaded_modules = Vec::with_capacity(4);
        let mut module_url = String::with_capacity(512);
        module_url.push_str("https://cdn.jsdelivr.net/npm/example@1.0.0/+esm");
        let mut module_hash = String::with_capacity(128);
        module_hash.push_str("abc123");
        loaded_modules.push((module_url, module_hash));

        let code = CachedActionCode {
            code: code_string,
            loaded_modules,
        };

        let size = action_code_cache_entry_size(&action_ipfs_id, action_ipfs_id.capacity(), &code);
        let minimum_allocated_bytes = action_ipfs_id.capacity()
            + code.code.capacity()
            + code.loaded_modules.capacity() * size_of::<(String, String)>()
            + code.loaded_modules[0].0.capacity()
            + code.loaded_modules[0].1.capacity();

        assert!(size >= minimum_allocated_bytes);
    }

    #[test]
    fn action_code_cache_expires_entries() {
        let now = Instant::now();
        let code = cached_code("console.log('old')");
        let action_ipfs_id = "QmOld".to_string();
        let size_bytes =
            action_code_cache_entry_size(&action_ipfs_id, action_ipfs_id.capacity(), &code);
        let mut cache = ActionCodeCacheState {
            entries: HashMap::from([(
                "QmOld".to_string(),
                ActionCodeCacheEntry {
                    code,
                    size_bytes,
                    expires_at: now,
                },
            )]),
            total_bytes: size_bytes,
        };

        assert!(cache.get("QmOld", now).is_none());
        cache.purge_expired(now);
        assert_eq!(cache.total_bytes(), 0);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn action_code_cache_enforces_total_size_after_purging_expired_entries() {
        let now = Instant::now();
        let existing_code = cached_code("old");
        let existing_size = MAX_ACTION_CODE_CACHE_BYTES - 1;
        let mut cache = ActionCodeCacheState {
            entries: HashMap::from([(
                "QmOld".to_string(),
                ActionCodeCacheEntry {
                    code: existing_code,
                    size_bytes: existing_size,
                    expires_at: now + ACTION_CODE_CACHE_TTL,
                },
            )]),
            total_bytes: existing_size,
        };

        assert!(!cache.insert("QmNew".to_string(), cached_code("new"), now));
        assert_eq!(cache.total_bytes(), existing_size);

        if let Some(entry) = cache.entries.get_mut("QmOld") {
            entry.expires_at = now;
        }

        assert!(cache.insert("QmNew".to_string(), cached_code("new"), now));
        assert!(cache.total_bytes() < MAX_ACTION_CODE_CACHE_BYTES);
        assert!(cache.entries.contains_key("QmNew"));
        assert!(!cache.entries.contains_key("QmOld"));
    }

    /// Each `PreparedWorker` must have a fresh `LoadedModules` `Arc`.
    /// If two workers ever shared the same accumulator, per-tenant module
    /// state would leak between the requests dispatched to those workers.
    /// This is the core safety invariant for the pool.
    #[test]
    fn prepared_workers_have_distinct_loaded_modules_arcs() {
        use std::sync::Once;
        static INIT_V8_ONCE: Once = Once::new();
        INIT_V8_ONCE.call_once(super::init_v8);

        let shared = PoolSharedState {
            integrity_manifest: Arc::new(RwLock::new(HashMap::new())),
            strict_imports: false,
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            lockfile_path: None,
            http_client: CdnModuleLoader::build_http_client(),
            memory_limit_mb: DEFAULT_MEMORY_LIMIT_MB,
        };

        let w1 = build_worker_base(&shared).expect("first worker built");
        let w2 = build_worker_base(&shared).expect("second worker built");

        let m1 = w1.loaded_modules_for_test();
        let m2 = w2.loaded_modules_for_test();

        assert!(
            !Arc::ptr_eq(&m1.0, &m2.0),
            "two PreparedWorkers must not share a LoadedModules Arc; \
             this would leak per-request module state between tenants",
        );
    }
}
