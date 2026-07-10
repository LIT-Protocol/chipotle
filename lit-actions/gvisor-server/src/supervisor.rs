//! One execution end-to-end: bundle resolve → per-exec guest-ops socket →
//! sandbox spawn → wait (timeout / usage ticks / fatal op errors) → teardown.
//!
//! Mirrors the JS runner's `runtime::execute_with_worker` control flow, with
//! the sandbox process standing in for the V8 isolate: termination is "the
//! entrypoint exits" (serverless semantics), `SetResponse` merely records
//! the response api-server-side, and timeout/cancel are hard kills reported
//! with the same `tonic::Status` codes and messages as the JS runner.

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
use crate::bundle::{Bundle, BundleCache};
use crate::guest_service::GuestOpsService;
use crate::proto::{GuestOpsServer, Job};
use crate::sandbox::{ENV_ACTION_IPFS_ID, ExecSpec, OP_SOCK_FILE, SandboxRuntime};

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

        // Guest-ops server on the per-exec socket. Bound BEFORE the sandbox
        // spawns so the guest never races the listener.
        let listener = UnixListener::bind(&sock_path)
            .with_context(|| format!("failed to bind op socket {}", sock_path.display()))?;
        // The guest may run as any uid; the socket sits in a 0700 host
        // tempdir, so host-side exposure is unchanged.
        std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o777))?;

        let job = Job {
            js_params: req.js_params,
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
            exec_dir.path().to_path_buf(),
            memory_limit_mb,
        )?;

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

fn build_spec(
    bundle: &Bundle,
    id: String,
    sock_dir: std::path::PathBuf,
    exec_dir: std::path::PathBuf,
    memory_limit_mb: u64,
) -> Result<ExecSpec> {
    let mut env: Vec<(String, String)> = bundle
        .manifest
        .env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    env.push((ENV_ACTION_IPFS_ID.to_string(), bundle.cid.clone()));

    Ok(ExecSpec {
        id,
        bundle_dir: bundle.dir.clone(),
        argv: bundle.manifest.entrypoint.to_argv()?,
        env,
        sock_dir,
        exec_dir,
        memory_limit_mb,
        pids_limit: DEFAULT_PIDS_LIMIT,
        tmpfs_mb: DEFAULT_TMPFS_MB,
    })
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
