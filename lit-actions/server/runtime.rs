use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Once;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use deno_core::{JsRuntime, ModuleSpecifier, v8};

use crate::cdn_module_loader::{CdnModuleLoader, ModuleCache};
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

/// Returns true if the code contains static ES `import` or `export` declarations.
/// Dynamic `import()` calls are excluded since they work in script mode.
///
/// This is a token-based heuristic that scans for `import`/`export` at statement
/// boundaries (start of source or after `;`), while skipping comments and string
/// literals. This handles minified single-line ESM like `;import{...}from"x"`.
fn has_static_module_syntax(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    let mut at_boundary = true; // start of source is a statement boundary

    while i < bytes.len() {
        let b = bytes[i];

        // Whitespace preserves boundary state
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Skip comments
        if b == b'/' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'/' => {
                    // Single-line comment: skip to end of line
                    i += 2;
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                b'*' => {
                    // Block comment: skip to */
                    i += 2;
                    while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    if i + 1 < bytes.len() {
                        i += 2;
                    }
                    continue;
                }
                _ => {}
            }
        }

        // Skip string literals (single, double, and template)
        if matches!(b, b'\'' | b'"' | b'`') {
            let quote = b;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2; // skip escaped character
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            at_boundary = false;
            continue;
        }

        // Semicolons and braces mark statement boundaries
        if matches!(b, b';' | b'{' | b'}') {
            at_boundary = true;
            i += 1;
            continue;
        }

        // Check for `import` or `export` at a statement boundary
        if at_boundary {
            if bytes[i..].starts_with(b"import")
                && i + 6 <= bytes.len()
                && !bytes.get(i + 6).copied().is_some_and(is_js_ident_char)
            {
                // Skip whitespace/comments after `import` to check for `(`
                let next = skip_trivia(bytes, i + 6);
                if next >= bytes.len() || bytes[next] != b'(' {
                    return true;
                }
            }

            if bytes[i..].starts_with(b"export")
                && i + 6 <= bytes.len()
                && !bytes.get(i + 6).copied().is_some_and(is_js_ident_char)
            {
                return true;
            }
        }

        at_boundary = false;
        i += 1;
    }
    false
}

/// Returns true if the byte can continue a JS identifier (alphanumeric, _, $).
fn is_js_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$')
}

/// Skip whitespace and comments, returning the index of the next meaningful byte.
fn skip_trivia(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'/' => {
                    i += 2;
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                b'*' => {
                    i += 2;
                    while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    if i + 1 < bytes.len() {
                        i += 2;
                    }
                    continue;
                }
                _ => {}
            }
        }
        break;
    }
    i
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
    )
    .context("Error building main worker")
    .map_err(|e| anyhow!("{e:#}"))?; // Ensure to keep context when downcasting JS errors later

    let op_state = worker.js_runtime.op_state();
    {
        // scope the borrow
        let mut state = op_state.borrow_mut();
        state.put(outbound_tx);
        state.put(inbound_rx);
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

    let uses_modules = has_static_module_syntax(&code);

    // When the user code contains static import/export, we must evaluate it as
    // an ES module (module mode) instead of a script so the `import` syntax is valid.
    // Module loading (which fetches CDN imports) happens inside the timeout-governed
    // select loop so that slow/hanging imports are properly terminated.
    let maybe_mod_eval = if uses_modules {
        let module_code = format!(
            "
        {code}
        ;
        const __lit_mod_params__ = {js_func_params} ;
        const __lit_mod_result__ = await main(__lit_mod_params__);
        if (typeof __lit_mod_result__ !== \"undefined\") {{
          LitActions.setResponse( {{ response: __lit_mod_result__ }} );
        }}"
        );
        let specifier = ModuleSpecifier::parse("lit:user_action").expect("valid specifier");

        // Load the module inside the timeout loop so CDN fetches are bounded
        let load_fut = worker
            .js_runtime
            .load_main_es_module_from_code(&specifier, module_code);

        let mod_id = tokio::select! {
            biased;
            execution_result = &mut halt_isolate_rx => {
                match execution_result {
                    Ok(ExecutionResult::Timeout) => bail!(Status::deadline_exceeded(format!("Your function exceeded the maximum runtime of {timeout_ms}ms and was terminated."))),
                    Ok(ExecutionResult::OutOfMemory) => bail!(Status::resource_exhausted(format!("Your function exceeded the maximum memory of {memory_limit_mb} MB and was terminated."))),
                    _ => bail!("Module loading interrupted"),
                }
            }
            result = load_fut => {
                match result {
                    Ok(mod_id) => mod_id,
                    Err(e) => {
                        // Mirror the script path: if the controller terminated the isolate,
                        // defer to halt_isolate_rx for the proper timeout/OOM error.
                        if e.to_string() != EXECUTION_TERMINATED_ERROR {
                            return Err(e).context("Error loading user action as ES module");
                        }
                        match halt_isolate_rx.await {
                            Ok(ExecutionResult::Timeout) => bail!(Status::deadline_exceeded(format!("Your function exceeded the maximum runtime of {timeout_ms}ms and was terminated."))),
                            Ok(ExecutionResult::OutOfMemory) => bail!(Status::resource_exhausted(format!("Your function exceeded the maximum memory of {memory_limit_mb} MB and was terminated."))),
                            _ => bail!("Module loading interrupted"),
                        }
                    }
                }
            }
        };

        Some(worker.js_runtime.mod_evaluate(mod_id))
    } else {
        // Legacy path: execute as script (no static imports)
        let code = format!(
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
        );

        if let Err(e) = worker
            .js_runtime
            .execute_script("<user_provided_script>", code)
        {
            // Delay error handling if the controller has terminated the isolate,
            // in which case halt_isolate_rx will tell the reason.
            if e.to_string() != EXECUTION_TERMINATED_ERROR {
                bail!(e);
            }
        }

        None
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

    // For module evaluation, check the result after the event loop has driven it to completion
    if let Some(eval_fut) = maybe_mod_eval {
        eval_fut
            .await
            .context("Error evaluating user action module")?;
    }

    Ok(())
}

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
    fn detects_static_import_from() {
        assert!(has_static_module_syntax(
            r#"import { z } from "zod@3.22.4/+esm";"#
        ));
    }

    #[test]
    fn detects_static_import_default() {
        assert!(has_static_module_syntax(
            r#"import zod from "zod@3.22.4/+esm";"#
        ));
    }

    #[test]
    fn detects_static_import_side_effect() {
        assert!(has_static_module_syntax(r#"import "zod@3.22.4/+esm";"#));
    }

    #[test]
    fn detects_static_import_star() {
        assert!(has_static_module_syntax(
            r#"import * as z from "zod@3.22.4/+esm";"#
        ));
    }

    #[test]
    fn detects_export_declaration() {
        assert!(has_static_module_syntax("export function main() {}"));
    }

    #[test]
    fn ignores_dynamic_import() {
        assert!(!has_static_module_syntax(
            r#"const z = await import("zod");"#
        ));
    }

    #[test]
    fn ignores_dynamic_import_at_line_start() {
        assert!(!has_static_module_syntax(r#"import("zod")"#));
    }

    #[test]
    fn ignores_commented_import() {
        assert!(!has_static_module_syntax(r#"// import { z } from "zod";"#));
    }

    #[test]
    fn no_imports_returns_false() {
        assert!(!has_static_module_syntax(
            "async function main() { return 42; }"
        ));
    }

    #[test]
    fn mixed_code_with_import() {
        let code = r#"
import { z } from "zod@3.22.4/+esm";

async function main(params) {
    const schema = z.object({ name: z.string() });
    return schema.parse(params);
}
"#;
        assert!(has_static_module_syntax(code));
    }

    #[test]
    fn import_in_middle_of_code() {
        let code = r#"
const x = 1;
import { foo } from "bar@1.0.0/+esm";
async function main() { return x; }
"#;
        assert!(has_static_module_syntax(code));
    }

    #[test]
    fn detects_minified_import_braces() {
        assert!(has_static_module_syntax(
            r#"import{z}from"zod@3.22.4/+esm";"#
        ));
    }

    #[test]
    fn detects_minified_export_braces() {
        assert!(has_static_module_syntax(r#"export{main}"#));
    }

    #[test]
    fn detects_import_with_single_quotes() {
        assert!(has_static_module_syntax(r#"import'zod@3.22.4/+esm';"#));
    }

    #[test]
    fn detects_import_after_semicolon() {
        assert!(has_static_module_syntax(
            r#"var x = 1;import{z}from"zod@3.22.4/+esm";"#
        ));
    }

    #[test]
    fn ignores_block_comment_containing_import() {
        assert!(!has_static_module_syntax(
            r#"/* import { z } from "zod"; */ async function main() {}"#
        ));
    }

    #[test]
    fn ignores_string_literal_containing_import() {
        assert!(!has_static_module_syntax(
            r#"const s = "import foo from 'bar'"; async function main() {}"#
        ));
    }

    #[test]
    fn ignores_template_literal_containing_import() {
        assert!(!has_static_module_syntax(
            r#"const s = `import foo from 'bar'`; async function main() {}"#
        ));
    }

    #[test]
    fn detects_export_after_closing_brace() {
        assert!(has_static_module_syntax(
            "function helper() { return 1; } export { helper }"
        ));
    }
}
