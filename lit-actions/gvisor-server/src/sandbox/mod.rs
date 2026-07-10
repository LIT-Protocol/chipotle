//! Sandbox runtimes.
//!
//! `RunscRuntime` is the production runtime: one gVisor sandbox per
//! execution. `ProcessRuntime` runs the entrypoint as a plain child process
//! (NO isolation) for integration tests and local development on hosts
//! without gVisor. Both present the identical guest contract: same env vars,
//! same op-socket location, same argv.

pub mod process;
pub mod runsc;

use std::path::PathBuf;

use anyhow::Result;

pub use process::ProcessRuntime;
pub use runsc::{RunscConfig, RunscRuntime};

/// Guest-visible locations/env. Shared constants so the runtimes, the
/// supervisor, the `lit` CLI, and the docs cannot drift.
pub const GUEST_ACTION_DIR: &str = "/action";
pub const GUEST_SOCK_DIR: &str = "/run/lit";
pub const OP_SOCK_FILE: &str = "ops.sock";
pub const ENV_OP_SOCK: &str = "LIT_OP_SOCK";
pub const ENV_ACTION_IPFS_ID: &str = "LIT_ACTION_IPFS_ID";

/// Everything a runtime needs to run one sandboxed execution.
#[derive(Debug)]
pub struct ExecSpec {
    /// Unique execution id (also the runsc container id).
    pub id: String,
    /// Read-only unpacked bundle contents (shared cache — never write here).
    pub bundle_dir: PathBuf,
    /// Entrypoint argv from the bundle manifest.
    pub argv: Vec<String>,
    /// Extra environment (manifest env + LIT_* job vars).
    pub env: Vec<(String, String)>,
    /// Host dir containing the per-execution op socket (`ops.sock`).
    pub sock_dir: PathBuf,
    /// Per-execution scratch dir (OCI bundle, runsc state, work copies).
    pub exec_dir: PathBuf,
    pub memory_limit_mb: u64,
    pub pids_limit: u64,
    pub tmpfs_mb: u64,
}

pub trait SandboxRuntime: Send + Sync {
    fn name(&self) -> &'static str;

    /// Build the host command that runs the sandboxed entrypoint. The
    /// supervisor owns spawning, stdio, and killing the returned command.
    fn command(&self, spec: &ExecSpec) -> Result<tokio::process::Command>;

    /// Best-effort forced termination beyond killing the host process (e.g.
    /// `runsc kill`). `pid` is the host process id, when still known.
    fn terminate(&self, _spec: &ExecSpec, _pid: Option<u32>) {}

    /// Best-effort post-run teardown of runtime state (e.g. `runsc delete`).
    fn cleanup(&self, _spec: &ExecSpec) {}
}
