use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Once;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use deno_core::{JsRuntime, v8};

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

/// Maximum number of entries in the code rewrite cache.
const CODE_CACHE_MAX_ENTRIES: usize = 1_000;

/// Cached result of import rewriting for a given IPFS CID.
#[derive(Clone)]
pub(crate) struct CachedRewrite {
    /// SHA-256 digest of the original code, used to verify cache integrity.
    pub code_hash: [u8; 32],
    /// User code with import statements stripped.
    pub rewritten_code: String,
    /// Pre-rendered dynamic import statements (empty string when no imports).
    pub dynamic_imports: String,
    /// Whether the original code had imports.
    pub has_imports: bool,
}

/// Thread-safe cache mapping IPFS CID → rewrite result.
pub(crate) type CodeCache = Arc<RwLock<HashMap<String, CachedRewrite>>>;

/// Compute a SHA-256 hash of the code for cache integrity verification.
fn hash_code(code: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(code.as_bytes());
    hasher.finalize().into()
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

// using the worker built into deno
#[allow(clippy::too_many_arguments)]
#[instrument(skip_all, err)]
fn build_main_worker_and_inject_sdk(
    globals_to_inject: &Option<serde_json::Value>,
    auth_context: &Option<serde_json::Value>,
    http_headers: BTreeMap<String, String>,
    memory_limit_mb: Option<usize>,
    integrity_manifest: Arc<RwLock<HashMap<String, String>>>,
    strict_imports: bool,
    module_cache: ModuleCache,
    lockfile_path: Option<PathBuf>,
    http_client: Arc<reqwest::Client>,
    loaded_modules: LoadedModules,
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
            module_loader: Rc::new(CdnModuleLoader::with_options(
                integrity_manifest,
                strict_imports,
                module_cache,
                lockfile_path,
                Some(http_client),
                loaded_modules,
            )),
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
    lockfile_path: Option<PathBuf>,
    http_client: Arc<reqwest::Client>,
    ipfs_id: Option<String>,
    code_cache: CodeCache,
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

    let loaded_modules = LoadedModules::default();

    let mut worker = build_main_worker_and_inject_sdk(
        &None,
        &auth_context,
        http_headers,
        Some(memory_limit_mb),
        integrity_manifest,
        strict_imports,
        module_cache,
        lockfile_path,
        http_client,
        loaded_modules.clone(),
    )
    .context("Error building main worker")
    .map_err(|e| anyhow!("{e:#}"))?; // Ensure to keep context when downcasting JS errors later

    let op_state = worker.js_runtime.op_state();
    {
        // scope the borrow
        let mut state = op_state.borrow_mut();
        state.put(outbound_tx);
        state.put(inbound_rx);
        state.put(loaded_modules);
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

    // Try to use cached rewrite result keyed by IPFS CID to skip import parsing.
    // On cache hit, verify the code hash to prevent cache poisoning from mismatched CIDs.
    // Only compute the hash when we have a CID (avoids O(n) hashing on every call).
    let cached = ipfs_id.as_deref().and_then(|cid| {
        code_cache
            .read()
            .ok()
            .and_then(|cache| cache.get(cid).cloned())
            .filter(|entry| entry.code_hash == hash_code(&code))
    });

    let (rewritten_code, dynamic_imports, has_imports) = if let Some(cached) = cached {
        debug!("Code cache hit for IPFS CID");
        (
            cached.rewritten_code,
            cached.dynamic_imports,
            cached.has_imports,
        )
    } else {
        // Rewrite static ES `import` statements into dynamic `import()` calls.
        // Static imports are not valid in script mode, so we strip them from the
        // user code and generate equivalent dynamic imports inside the async wrapper
        // where `await` is available and the imported bindings are in lexical scope.
        let import_rewriter::RewriteResult {
            code: rewritten_code,
            imports,
        } = import_rewriter::rewrite_imports(&code);

        let has_imports = !imports.is_empty();
        let dynamic_imports = if has_imports {
            import_rewriter::generate_dynamic_imports(&imports)
        } else {
            String::new()
        };

        // Cache the rewrite result for future calls with the same IPFS CID.
        // Use the entry API so existing CIDs can always be updated (e.g. code changed),
        // while new inserts are blocked once the cache is at capacity.
        if let Some(cid) = ipfs_id.as_deref()
            && let Ok(mut cache) = code_cache.write()
        {
            let cached_rewrite = CachedRewrite {
                code_hash: hash_code(&code),
                rewritten_code: rewritten_code.clone(),
                dynamic_imports: dynamic_imports.clone(),
                has_imports,
            };

            // Check capacity before borrowing via entry() to avoid
            // simultaneous mutable + immutable borrow of the HashMap.
            let at_capacity = cache.len() >= CODE_CACHE_MAX_ENTRIES;
            match cache.entry(cid.to_string()) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    entry.insert(cached_rewrite);
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    if !at_capacity {
                        entry.insert(cached_rewrite);
                    } else {
                        debug!(
                            "Code cache full ({CODE_CACHE_MAX_ENTRIES} entries), skipping insert"
                        );
                    }
                }
            }
        }

        (rewritten_code, dynamic_imports, has_imports)
    };

    let code = if !has_imports {
        // No imports — preserve the existing wrapper layout unchanged.
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
        }})();"
        )
    } else {
        // Has imports — move user code inside the async IIFE so that the
        // dynamically imported bindings are in lexical scope for main().
        format!(
            "
        (async () => {{
        {dynamic_imports}
        {rewritten_code}
        ;
        const params = {js_func_params} ;
        const data = await main(params);
        if (typeof data !== \"undefined\") {{
          LitActions.setResponse( {{ response: data }} );
        }}
        }})();"
        )
    };

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

    #[test]
    fn hash_code_is_deterministic() {
        let code = "async function main() { return 42; }";
        assert_eq!(hash_code(code), hash_code(code));
    }

    #[test]
    fn hash_code_differs_for_different_input() {
        let a = hash_code("async function main() { return 1; }");
        let b = hash_code("async function main() { return 2; }");
        assert_ne!(a, b);
    }

    #[test]
    fn code_cache_hit_returns_cached_entry() {
        let cache: CodeCache = Arc::new(RwLock::new(HashMap::new()));
        let code = "async function main() {}";
        let digest = hash_code(code);

        cache.write().unwrap().insert(
            "QmTest123".to_string(),
            CachedRewrite {
                code_hash: digest,
                rewritten_code: code.to_string(),
                dynamic_imports: String::new(),
                has_imports: false,
            },
        );

        let result = cache
            .read()
            .ok()
            .and_then(|c| c.get("QmTest123").cloned())
            .filter(|entry| entry.code_hash == digest);

        assert!(result.is_some());
        assert!(!result.unwrap().has_imports);
    }

    #[test]
    fn code_cache_rejects_mismatched_hash() {
        let cache: CodeCache = Arc::new(RwLock::new(HashMap::new()));
        let original_code = "async function main() { return 1; }";
        let different_code = "async function main() { return 2; }";

        cache.write().unwrap().insert(
            "QmTest456".to_string(),
            CachedRewrite {
                code_hash: hash_code(original_code),
                rewritten_code: original_code.to_string(),
                dynamic_imports: String::new(),
                has_imports: false,
            },
        );

        // Lookup with a different code should fail the hash check
        let different_digest = hash_code(different_code);
        let result = cache
            .read()
            .ok()
            .and_then(|c| c.get("QmTest456").cloned())
            .filter(|entry| entry.code_hash == different_digest);

        assert!(result.is_none());
    }

    /// Helper that mirrors the entry-based insert logic used in execute_js.
    fn cache_insert(cache: &CodeCache, cid: &str, entry: CachedRewrite) -> bool {
        if let Ok(mut c) = cache.write() {
            let at_capacity = c.len() >= CODE_CACHE_MAX_ENTRIES;
            match c.entry(cid.to_string()) {
                std::collections::hash_map::Entry::Occupied(mut slot) => {
                    slot.insert(entry);
                    true
                }
                std::collections::hash_map::Entry::Vacant(slot) => {
                    if !at_capacity {
                        slot.insert(entry);
                        true
                    } else {
                        false
                    }
                }
            }
        } else {
            false
        }
    }

    #[test]
    fn code_cache_respects_max_entries() {
        let cache: CodeCache = Arc::new(RwLock::new(HashMap::new()));

        // Fill the cache to capacity
        {
            let mut c = cache.write().unwrap();
            for i in 0..CODE_CACHE_MAX_ENTRIES {
                c.insert(
                    format!("Qm{i}"),
                    CachedRewrite {
                        code_hash: [0u8; 32],
                        rewritten_code: String::new(),
                        dynamic_imports: String::new(),
                        has_imports: false,
                    },
                );
            }
        }

        assert_eq!(cache.read().unwrap().len(), CODE_CACHE_MAX_ENTRIES);

        // New CID insert should be rejected when full
        let inserted = cache_insert(
            &cache,
            "QmOverflow",
            CachedRewrite {
                code_hash: [0u8; 32],
                rewritten_code: String::new(),
                dynamic_imports: String::new(),
                has_imports: false,
            },
        );

        assert!(!inserted);
        assert_eq!(cache.read().unwrap().len(), CODE_CACHE_MAX_ENTRIES);
        assert!(!cache.read().unwrap().contains_key("QmOverflow"));
    }

    #[test]
    fn code_cache_allows_update_when_full() {
        let cache: CodeCache = Arc::new(RwLock::new(HashMap::new()));

        // Fill the cache to capacity, including a known CID "QmExisting"
        {
            let mut c = cache.write().unwrap();
            c.insert(
                "QmExisting".to_string(),
                CachedRewrite {
                    code_hash: [1u8; 32],
                    rewritten_code: "old".to_string(),
                    dynamic_imports: String::new(),
                    has_imports: false,
                },
            );
            for i in 1..CODE_CACHE_MAX_ENTRIES {
                c.insert(
                    format!("Qm{i}"),
                    CachedRewrite {
                        code_hash: [0u8; 32],
                        rewritten_code: String::new(),
                        dynamic_imports: String::new(),
                        has_imports: false,
                    },
                );
            }
        }

        assert_eq!(cache.read().unwrap().len(), CODE_CACHE_MAX_ENTRIES);

        // Updating an existing CID should succeed even when full
        let updated = cache_insert(
            &cache,
            "QmExisting",
            CachedRewrite {
                code_hash: [2u8; 32],
                rewritten_code: "new".to_string(),
                dynamic_imports: String::new(),
                has_imports: false,
            },
        );

        assert!(updated);
        assert_eq!(cache.read().unwrap().len(), CODE_CACHE_MAX_ENTRIES);
        let entry = cache.read().unwrap().get("QmExisting").cloned().unwrap();
        assert_eq!(entry.rewritten_code, "new");
        assert_eq!(entry.code_hash, [2u8; 32]);
    }

    #[test]
    fn code_cache_miss_returns_none() {
        let cache: CodeCache = Arc::new(RwLock::new(HashMap::new()));
        let result = cache
            .read()
            .ok()
            .and_then(|c| c.get("QmNonexistent").cloned());
        assert!(result.is_none());
    }
}
