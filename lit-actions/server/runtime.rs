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

use crate::cdn_module_loader::{CdnModuleLoader, DataUrlModuleLoader, LoadedModules, ModuleCache};
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
use sys_traits::impls::RealSys;
use tokio::sync::{mpsc, oneshot};
use tonic::Status;
use tracing::{debug, error, info_span, instrument};

// Same default limits as in lit-node's action client
const DEFAULT_TIMEOUT_MS: u64 = 1000 * 60 * 15; // 15 minutes
const DEFAULT_MEMORY_LIMIT_MB: usize = 128; // 128MB
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
    client: &'a reqwest::Client,
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
    code: String,
    dynamic_imports: Option<String>,
    loaded_modules: Vec<(String, String)>,
}

impl CachedActionCode {
    fn allocated_bytes(&self) -> usize {
        self.code.capacity()
            + self.dynamic_imports.as_ref().map_or(0, String::capacity)
            + self.loaded_modules.capacity() * size_of::<(String, String)>()
            + self
                .loaded_modules
                .iter()
                .map(|(url, hash)| url.capacity() + hash.capacity())
                .sum::<usize>()
    }

    fn to_executable_code(&self, js_func_params: &str) -> String {
        if let Some(dynamic_imports) = &self.dynamic_imports {
            // Has imports - move user code inside the async IIFE so that the
            // dynamically imported bindings are in lexical scope for main().
            format!(
                "
        (async () => {{
        {dynamic_imports}
        {code}
        ;
        const params = {js_func_params} ;
        const data = await main(params);
        if (typeof data !== \"undefined\") {{
          LitActions.setResponse( {{ response: data }} );
        }}
        }})();",
                code = self.code,
            )
        } else {
            // No imports - preserve the existing wrapper layout unchanged.
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
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum ExecutionResult {
    Complete,
    Timeout,
    OutOfMemory,
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
    // Rewrite static ES `import` statements into dynamic `import()` calls.
    // Static imports are not valid in script mode, so we strip them from the
    // user code and generate equivalent dynamic imports inside the async wrapper
    // where `await` is available and the imported bindings are in lexical scope.
    let import_rewriter::RewriteResult {
        code: rewritten_code,
        imports,
    } = import_rewriter::rewrite_imports(code);

    if imports.is_empty() {
        return Ok(CachedActionCode {
            code: code.to_string(),
            dynamic_imports: None,
            loaded_modules: Vec::new(),
        });
    }

    // Has imports - bundle all transitive dependencies from jsDelivr as
    // data: URLs so the module loader needs no network I/O at runtime.
    let bundle_result = import_rewriter::bundle_imports(
        &imports,
        context.client,
        context.integrity,
        context.strict_imports,
        context.lockfile_path,
        context.module_cache,
    )
    .await
    .map_err(|e| anyhow!("Failed to bundle CDN imports: {e}"))?;

    Ok(CachedActionCode {
        code: rewritten_code,
        dynamic_imports: Some(bundle_result.dynamic_imports),
        loaded_modules: bundle_result.loaded_modules,
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

// using the worker built into deno
#[allow(clippy::too_many_arguments)]
#[instrument(skip_all, err)]
fn build_main_worker_and_inject_sdk(
    globals_to_inject: &Option<serde_json::Value>,
    auth_context: &Option<serde_json::Value>,
    http_headers: BTreeMap<String, String>,
    memory_limit_mb: Option<usize>,
    module_loader: Rc<dyn deno_core::ModuleLoader>,
) -> Result<MainWorker> {
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
        create_params: memory_limit_mb
            .map(|limit| v8::CreateParams::default().heap_limits(0, limit * 1024 * 1024)),
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
    let mut worker = MainWorker::bootstrap_from_options(&main_module, services, options);

    {
        let _span = info_span!("LitNamespace.js").entered();

        if http_headers
            .get(&HEADER_KEY_X_PRIVACY_MODE.to_ascii_lowercase())
            .unwrap_or(&"false".to_string())
            == "true"
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
            headers = serde_json::to_string(&http_headers).context("Could not serialize HTTP headers")?
        };

        // Remove LitTest from non-debug builds
        if !cfg!(debug_assertions) {
            code.push_str("delete globalThis.LitTest;\n");
        }

        worker
            .execute_script("LitNamespace.js", code.into())
            .context("Error populating Lit namespace")?;
    }

    {
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
    }

    if let Some(params) = globals_to_inject {
        let _span = info_span!("Params.js").entered();

        if http_headers
            .get(&HEADER_KEY_X_PRIVACY_MODE.to_ascii_lowercase())
            .unwrap_or(&"false".to_string())
            == "true"
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
    }

    Ok(worker)
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

    let timeout_ms = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let memory_limit_mb = memory_limit_mb.unwrap_or(DEFAULT_MEMORY_LIMIT_MB);

    // Check the action code cache before building the worker. On cache hit
    // we use a lightweight DataUrlModuleLoader that only handles the bundled
    // data:text/javascript URIs — no HTTP client, integrity manifest, or CDN
    // logic needed. On cache miss we need the full CdnModuleLoader.
    let action_ipfs_id = get_lit_action_ipfs_id(&code);
    let now = Instant::now();
    let cached_code = action_code_cache
        .read()
        .ok()
        .and_then(|cache| cache.get(&action_ipfs_id, now));

    let loaded_modules = LoadedModules::default();
    let module_loader: Rc<dyn deno_core::ModuleLoader> = if cached_code.is_some() {
        debug!(action_ipfs_id, "cache hit — using DataUrlModuleLoader");
        Rc::new(DataUrlModuleLoader)
    } else {
        Rc::new(CdnModuleLoader::with_options(
            integrity_manifest.clone(),
            strict_imports,
            module_cache.clone(),
            lockfile_path.clone(),
            Some(http_client.clone()),
            loaded_modules.clone(),
        ))
    };

    let mut worker = build_main_worker_and_inject_sdk(
        &None,
        &auth_context,
        http_headers,
        Some(memory_limit_mb),
        module_loader,
    )
    .context("Error building main worker")
    .map_err(|e| anyhow!("{e:#}"))?; // Ensure to keep context when downcasting JS errors later

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

    // On cache hit we already have the prepared code; on miss we need to
    // prepare it (rewrite imports, bundle dependencies, cache the result).
    let cached_code = if let Some(cached) = cached_code {
        cached
    } else {
        let prepare_context = ActionCodePrepareContext {
            client: &http_client,
            integrity: &integrity_manifest,
            strict_imports,
            lockfile_path: &lockfile_path,
            module_cache: &module_cache,
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
            dynamic_imports: None,
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

        let mut dynamic_imports = String::with_capacity(256);
        dynamic_imports.push_str("await import(\"data:text/javascript,\");");

        let mut loaded_modules = Vec::with_capacity(4);
        let mut module_url = String::with_capacity(512);
        module_url.push_str("https://cdn.jsdelivr.net/npm/example@1.0.0/+esm");
        let mut module_hash = String::with_capacity(128);
        module_hash.push_str("abc123");
        loaded_modules.push((module_url, module_hash));

        let code = CachedActionCode {
            code: code_string,
            dynamic_imports: Some(dynamic_imports),
            loaded_modules,
        };

        let size = action_code_cache_entry_size(&action_ipfs_id, action_ipfs_id.capacity(), &code);
        let minimum_allocated_bytes = action_ipfs_id.capacity()
            + code.code.capacity()
            + code.dynamic_imports.as_ref().unwrap().capacity()
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
}
