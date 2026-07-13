//! One execution end-to-end: bundle resolve → startup script materialize →
//! per-exec guest-ops socket → sandbox spawn → wait (timeout / usage ticks /
//! fatal op errors) → teardown.
//!
//! Mirrors the JS runner's `runtime::execute_with_worker` control flow, with
//! the sandbox process standing in for the V8 isolate: termination is "the
//! startup script exits" (serverless semantics), `SetResponse` merely records
//! the response api-server-side, and timeout/cancel are hard kills reported
//! with the same `tonic::Status` codes and messages as the JS runner.
//!
//! The sandbox only ever executes `bash startup.sh` (CPL-355). The script
//! comes from the request when supplied — so one cached bundle serves many
//! different startup scripts — falling back to a `startup.sh` at the bundle
//! root. Authorization stays keyed on the bundle CID alone; the per-request
//! script is authenticated by API-key authorization, not content-addressing.

use std::os::unix::fs::PermissionsExt as _;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use lit_actions_grpc::proto::*;
use tokio::io::{AsyncBufReadExt as _, AsyncRead, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::Status;
use tracing::{debug, error, instrument};

use crate::bridge::OpBridge;
use crate::bundle::{Bundle, BundleCache, STARTUP_SCRIPT_FILE};
use crate::guest_service::GuestOpsService;
use crate::proto::{GuestOpsServer, Job};
use crate::sandbox::{ENV_ACTION_IPFS_ID, ENV_OP_SOCK, ExecSpec, OP_SOCK_FILE, SandboxRuntime};

/// Same default as the JS runner.
pub const DEFAULT_TIMEOUT_MS: u64 = 1000 * 60 * 15; // 15 minutes
/// Hard ceiling for caller-supplied timeouts.
pub const MAX_TIMEOUT_MS: u64 = 1000 * 60 * 150; // 150 minutes
/// A whole language runtime needs more than the JS isolate's 64 MB.
pub const DEFAULT_MEMORY_LIMIT_MB: u64 = 512;
pub const MAX_MEMORY_LIMIT_MB: u64 = 2048;
pub const DEFAULT_PIDS_LIMIT: u64 = 128;
pub const DEFAULT_TMPFS_MB: u64 = 512;
/// Mirrors `MEMORY_SAMPLE_INTERVAL_MS` in the JS runner; lit-api-server
/// aggregates ticks and flushes billing every 5 accumulated seconds.
pub const USAGE_TICK_INTERVAL_MS: u64 = 500;

/// How much trailing stderr to include in a failure result.
const STDERR_TAIL_BYTES: usize = 8 * 1024;

/// Per-value / total caps on js-params injected into the sandbox
/// environment. Oversized values are skipped (still available via
/// `lit params`) rather than risking an `execve` E2BIG inside the sandbox.
const MAX_PARAM_ENV_VALUE_BYTES: usize = 64 * 1024;
const MAX_PARAM_ENV_TOTAL_BYTES: usize = 1024 * 1024;

/// Environment variables the runtime owns; js-params may not shadow them
/// (a param named PATH must not break `lit` resolution inside the guest).
const RESERVED_ENV: [&str; 5] = ["PATH", "HOME", "TMPDIR", ENV_OP_SOCK, ENV_ACTION_IPFS_ID];

pub struct Supervisor {
    runtime: Arc<dyn SandboxRuntime>,
    bundle_cache: BundleCache,
    /// Monotonic per-process counter making execution ids unique.
    exec_seq: AtomicU64,
}

impl Supervisor {
    pub fn new(runtime: Arc<dyn SandboxRuntime>, bundle_cache: BundleCache) -> Self {
        Self {
            runtime,
            bundle_cache,
            exec_seq: AtomicU64::new(0),
        }
    }

    /// Run one `ExecutionRequest` to completion. `Ok(())` means the
    /// entrypoint exited 0; errors are reported to the caller either as a
    /// failed `ExecutionResult` or, for `tonic::Status` errors
    /// (timeout/cancel), as a stream error — exactly like the JS runner.
    #[instrument(skip_all, fields(exec_id))]
    pub async fn run_execution(&self, req: ExecutionRequest, bridge: Arc<OpBridge>) -> Result<()> {
        let timeout_ms = req
            .timeout
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);
        let memory_limit_mb = req
            .memory_limit
            .map_or(DEFAULT_MEMORY_LIMIT_MB, u64::from)
            .min(MAX_MEMORY_LIMIT_MB);

        // Resolve/unpack the bundle off the async runtime (tar/gzip file IO).
        let bundle = {
            let cache = self.bundle_cache.clone();
            let code = req.code;
            let ipfs_id = req.ipfs_id.clone();
            tokio::task::spawn_blocking(move || cache.resolve(&code, ipfs_id.as_deref()))
                .await
                .context("bundle resolve task panicked")??
        };

        let exec_id = format!(
            "lit-exec-{}-{}",
            std::process::id(),
            self.exec_seq.fetch_add(1, Ordering::Relaxed)
        );
        tracing::Span::current().record("exec_id", exec_id.as_str());
        debug!(
            cid = bundle.cid,
            timeout_ms, memory_limit_mb, "starting execution"
        );

        // Per-exec scratch dir; auto-removed on drop.
        let exec_dir = tempfile::tempdir().context("failed to create exec dir")?;
        let sock_dir = exec_dir.path().join("sock");
        std::fs::create_dir_all(&sock_dir)?;
        let sock_path = sock_dir.join(OP_SOCK_FILE);

        // Materialize the startup script — the only thing the sandbox will
        // execute. The request-supplied script wins (one cached bundle, many
        // scripts); otherwise the bundle must ship one at its root.
        let startup_dir = exec_dir.path().join("startup");
        std::fs::create_dir_all(&startup_dir)?;
        match req
            .startup_script
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            Some(script) => {
                std::fs::write(startup_dir.join(STARTUP_SCRIPT_FILE), script)
                    .context("failed to write request startup script")?;
            }
            None => {
                let bundled = bundle.dir.join(STARTUP_SCRIPT_FILE);
                if !bundled.is_file() {
                    bail!(
                        "nothing to execute: the request supplied no startup script and the \
                         bundle has no {STARTUP_SCRIPT_FILE} at its root"
                    );
                }
                std::fs::copy(&bundled, startup_dir.join(STARTUP_SCRIPT_FILE))
                    .context("failed to stage bundle startup script")?;
            }
        }

        // Guest-ops server on the per-exec socket. Bound BEFORE the sandbox
        // spawns so the guest never races the listener.
        let listener = UnixListener::bind(&sock_path)
            .with_context(|| format!("failed to bind op socket {}", sock_path.display()))?;
        // The guest may run as any uid; the socket sits in a 0700 host
        // tempdir, so host-side exposure is unchanged.
        std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o777))?;

        let job = Job {
            js_params: req.js_params.clone(),
            auth_context: req.auth_context,
            http_headers: req.http_headers,
            ipfs_id: bundle.cid.clone(),
            timeout_ms,
        };
        let (guest_shutdown_tx, guest_shutdown_rx) = oneshot::channel::<()>();
        let guest_server = tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(GuestOpsServer::new(GuestOpsService {
                    bridge: bridge.clone(),
                    job,
                }))
                .serve_with_incoming_shutdown(UnixListenerStream::new(listener), async {
                    let _ = guest_shutdown_rx.await;
                }),
        );

        let spec = build_spec(
            &bundle,
            exec_id,
            sock_dir,
            startup_dir,
            exec_dir.path().to_path_buf(),
            memory_limit_mb,
            req.js_params.as_deref(),
        );

        let run_result = self.spawn_and_wait(&spec, &bridge, timeout_ms).await;

        // Teardown before propagating any error.
        let _ = guest_shutdown_tx.send(());
        let _ = guest_server.await;
        self.runtime.cleanup(&spec);

        run_result
    }

    /// Spawn the sandbox and drive it to completion: usage ticks every
    /// 500ms, hard timeout, fatal op-loop errors, log forwarding.
    async fn spawn_and_wait(
        &self,
        spec: &ExecSpec,
        bridge: &Arc<OpBridge>,
        timeout_ms: u64,
    ) -> Result<()> {
        let started = tokio::time::Instant::now();

        // Baseline usage sample before user code starts (JS runner parity):
        // fast actions still produce at least one tick.
        usage_tick(bridge, started).await?;

        let mut cmd = self.runtime.command(spec)?;
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn {} sandbox", self.runtime.name()))?;
        let pid = child.id();

        // Forward guest stdout/stderr as Print ops (`logs` in the caller's
        // response), keeping a stderr tail for failure reporting. A Print
        // rejected by lit-api-server (log quota, ReportError) is fatal, like
        // a throwing console.log in the JS runner.
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        let (fatal_tx, mut fatal_rx) = mpsc::channel::<anyhow::Error>(2);
        let out_task = child
            .stdout
            .take()
            .map(|out| tokio::spawn(forward_output(out, bridge.clone(), None, fatal_tx.clone())));
        let err_task = child.stderr.take().map(|err| {
            tokio::spawn(forward_output(
                err,
                bridge.clone(),
                Some(stderr_tail.clone()),
                fatal_tx.clone(),
            ))
        });
        drop(fatal_tx);

        let mut tick = tokio::time::interval(Duration::from_millis(USAGE_TICK_INTERVAL_MS));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        tick.tick().await; // consume the immediate first tick (baseline already sent)

        let deadline = tokio::time::sleep(Duration::from_millis(timeout_ms));
        tokio::pin!(deadline);

        let run_result: Result<std::process::ExitStatus> = loop {
            tokio::select! {
                biased;

                status = child.wait() => {
                    break status.context("failed to await sandbox process");
                }

                _ = &mut deadline => {
                    self.kill(spec, pid, &mut child).await;
                    break Err(anyhow!(Status::deadline_exceeded(format!(
                        "Your function exceeded the maximum runtime of {timeout_ms}ms and was terminated."
                    ))));
                }

                Some(fatal) = fatal_rx.recv() => {
                    self.kill(spec, pid, &mut child).await;
                    break Err(fatal);
                }

                _ = tick.tick() => {
                    if let Err(e) = usage_tick(bridge, started).await {
                        self.kill(spec, pid, &mut child).await;
                        break Err(e);
                    }
                }
            }
        };

        // Drain the forwarders BEFORE reporting the result so a trailing
        // Print can never race behind the final ExecutionResult.
        if let Some(t) = out_task {
            let _ = t.await;
        }
        if let Some(t) = err_task {
            let _ = t.await;
        }

        let status = run_result?;

        // A Print rejected right as the child exited still fails the run
        // (parity with a throwing console.log).
        if let Ok(fatal) = fatal_rx.try_recv() {
            return Err(fatal);
        }

        if status.success() {
            Ok(())
        } else {
            let tail = stderr_tail
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            if tail.is_empty() {
                bail!("Action process exited with {status}");
            }
            bail!("Action process exited with {status}:\n{tail}");
        }
    }

    async fn kill(&self, spec: &ExecSpec, pid: Option<u32>, child: &mut tokio::process::Child) {
        self.runtime.terminate(spec, pid);
        let _ = child.kill().await;
    }
}

#[allow(clippy::too_many_arguments)]
fn build_spec(
    bundle: &Bundle,
    id: String,
    sock_dir: std::path::PathBuf,
    startup_dir: std::path::PathBuf,
    exec_dir: std::path::PathBuf,
    memory_limit_mb: u64,
    js_params: Option<&[u8]>,
) -> ExecSpec {
    // Layering: js-params first, then manifest env — a param name colliding
    // with a manifest variable must not silently reconfigure the bundle —
    // and the runtime-owned LIT_* / RESERVED_ENV vars are never shadowed.
    let mut env = js_params_env(js_params);
    for (k, v) in &bundle.manifest.env {
        env.retain(|(name, _)| name != k);
        env.push((k.clone(), v.clone()));
    }
    env.push((ENV_ACTION_IPFS_ID.to_string(), bundle.cid.clone()));

    ExecSpec {
        id,
        bundle_dir: bundle.dir.clone(),
        startup_dir,
        env,
        sock_dir,
        exec_dir,
        memory_limit_mb,
        pids_limit: DEFAULT_PIDS_LIMIT,
        tmpfs_mb: DEFAULT_TMPFS_MB,
    }
}

/// Top-level js-params as environment variables for the startup script
/// (CPL-355): string values verbatim, everything else as compact JSON.
/// Params whose name is not a valid environment variable name, shadows a
/// runtime-owned variable, or whose value exceeds the size caps are skipped
/// — the full-fidelity data remains available via `lit params`.
fn js_params_env(js_params: Option<&[u8]>) -> Vec<(String, String)> {
    let Some(serde_json::Value::Object(params)) =
        js_params.and_then(|b| serde_json::from_slice(b).ok())
    else {
        return Vec::new();
    };

    let mut env = Vec::new();
    let mut total = 0usize;
    for (name, value) in params {
        if !is_valid_env_name(&name) || RESERVED_ENV.contains(&name.as_str()) {
            debug!(
                name,
                "js param not injected into env: reserved or invalid name"
            );
            continue;
        }
        let value = match value {
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        };
        if value.len() > MAX_PARAM_ENV_VALUE_BYTES
            || total + name.len() + value.len() > MAX_PARAM_ENV_TOTAL_BYTES
        {
            debug!(name, "js param not injected into env: too large");
            continue;
        }
        total += name.len() + value.len();
        env.push((name, value));
    }
    env
}

fn is_valid_env_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.as_bytes()[0].is_ascii_digit()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// One usage tick. Error messages match the JS runner verbatim (callers may
/// pattern-match them). `used_kb` is 0 for now: lit-api-server bills on wall
/// clock and ignores it; per-exec `memory.peak` metering is a cgroup
/// follow-up.
async fn usage_tick(bridge: &OpBridge, started: tokio::time::Instant) -> Result<()> {
    let resp = bridge
        .update_resource_usage(UpdateResourceUsageRequest {
            tick: started.elapsed().as_millis() as u32,
            used_kb: 0,
        })
        .await
        .map_err(|e| {
            error!("Error communicating with the lit-node to update resource usage: {e:?}");
            anyhow!(Status::resource_exhausted(
                "Error communicating with the lit-node to update resource usage.".to_string()
            ))
        })?;
    if resp.cancel_action {
        bail!(Status::resource_exhausted(
            "Your function ran out of funds to continue execution and was terminated.".to_string()
        ));
    }
    Ok(())
}

/// Line-forward a guest output stream as Print ops. Non-UTF-8 output stops
/// forwarding for that stream (the op-loop carries strings); the process
/// itself keeps running.
async fn forward_output<R: AsyncRead + Unpin>(
    stream: R,
    bridge: Arc<OpBridge>,
    tail: Option<Arc<Mutex<String>>>,
    fatal_tx: mpsc::Sender<anyhow::Error>,
) {
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(tail) = &tail {
            push_tail(tail, &line);
        }
        if let Err(e) = bridge
            .print(PrintRequest {
                message: format!("{line}\n"),
            })
            .await
        {
            let _ = fatal_tx
                .send(anyhow!("forwarding action output failed: {e:#}"))
                .await;
            return;
        }
    }
}

fn push_tail(tail: &Arc<Mutex<String>>, line: &str) {
    let mut tail = tail
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    tail.push_str(line);
    tail.push('\n');
    if tail.len() > STDERR_TAIL_BYTES {
        let cut = tail.len() - STDERR_TAIL_BYTES;
        // Trim on a char boundary to keep the String valid.
        let boundary = (cut..tail.len())
            .find(|i| tail.is_char_boundary(*i))
            .unwrap_or(tail.len());
        tail.drain(..boundary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(params: &str) -> Vec<(String, String)> {
        js_params_env(Some(params.as_bytes()))
    }

    #[test]
    fn params_env_strings_verbatim_other_types_as_json() {
        let env = env(r#"{"name":"lit","count":2,"flag":true,"obj":{"a":1},"nil":null}"#);
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("name"), Some("lit"));
        assert_eq!(get("count"), Some("2"));
        assert_eq!(get("flag"), Some("true"));
        assert_eq!(get("obj"), Some(r#"{"a":1}"#));
        assert_eq!(get("nil"), Some("null"));
    }

    #[test]
    fn params_env_skips_reserved_and_invalid_names() {
        let env =
            env(r#"{"PATH":"/evil","LIT_OP_SOCK":"x","not-a-name":"x","9lives":"x","ok":"yes"}"#);
        assert_eq!(env, vec![("ok".to_string(), "yes".to_string())]);
    }

    #[test]
    fn params_env_skips_oversized_values() {
        let big = "x".repeat(MAX_PARAM_ENV_VALUE_BYTES + 1);
        let env = env(&format!(r#"{{"big":"{big}","small":"y"}}"#));
        assert_eq!(env, vec![("small".to_string(), "y".to_string())]);
    }

    #[test]
    fn params_env_handles_non_object_and_absent() {
        assert!(js_params_env(None).is_empty());
        assert!(env("[1,2]").is_empty());
        assert!(env("not json").is_empty());
    }
}
