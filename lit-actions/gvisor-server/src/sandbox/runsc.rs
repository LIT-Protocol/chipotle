//! gVisor runtime: one `runsc` sandbox per execution.
//!
//! Spike-validated invariants (2026-07-01, Phala TDX dev CVMs — see the
//! build plan):
//! - `--host-uds=all` is required or the sandbox cannot reach the per-exec
//!   op socket (the default refuses host UDS). The socket we expose is
//!   per-sandbox, never a shared one.
//! - Nested in a container, each sandbox needs a delegated leaf cgroup or
//!   runsc hits cgroup-v2 `subtree_control: EBUSY`; `--ignore-cgroups`
//!   sidesteps this for dev/tests (per-exec limits are then not enforced by
//!   cgroups — supervisor timeout still applies).
//! - systrap (default) and ptrace platforms both work inside TDX; there is
//!   no /dev/kvm in the CVM, so the kvm platform is not an option.

use std::fs;
use std::path::PathBuf;
use std::process::Command as StdCommand;

use anyhow::{Context as _, Result, ensure};
use serde_json::json;
use tokio::process::Command;
use tracing::warn;

use super::{
    ENV_OP_SOCK, ExecSpec, GUEST_ACTION_DIR, GUEST_SOCK_DIR, OP_SOCK_FILE, SandboxRuntime,
};

#[derive(Debug, Clone)]
pub struct RunscConfig {
    /// Path to the `runsc` binary.
    pub runsc_path: PathBuf,
    /// Read-only base image rootfs shared by every sandbox (language
    /// runtimes + the preinstalled `lit` CLI), baked into the CVM image.
    pub rootfs: PathBuf,
    /// gVisor platform: `systrap` (default) or `ptrace`.
    pub platform: String,
    /// runsc network mode: `sandbox` (netstack), `host`, or `none`.
    pub network: String,
    /// Skip cgroup setup (dev/tests; see module docs).
    pub ignore_cgroups: bool,
}

pub struct RunscRuntime {
    cfg: RunscConfig,
}

impl RunscRuntime {
    pub fn new(cfg: RunscConfig) -> Result<Self> {
        ensure!(
            cfg.rootfs.is_dir(),
            "runsc rootfs {} is not a directory",
            cfg.rootfs.display()
        );
        Ok(Self { cfg })
    }

    /// Per-exec runsc state dir: isolates sandbox metadata per execution and
    /// dies with the exec dir.
    fn state_root(&self, spec: &ExecSpec) -> PathBuf {
        spec.exec_dir.join("runsc-state")
    }

    fn base_cmd(&self, spec: &ExecSpec) -> StdCommand {
        let mut cmd = StdCommand::new(&self.cfg.runsc_path);
        cmd.arg(format!("--root={}", self.state_root(spec).display()));
        cmd
    }
}

impl SandboxRuntime for RunscRuntime {
    fn name(&self) -> &'static str {
        "runsc"
    }

    fn command(&self, spec: &ExecSpec) -> Result<Command> {
        ensure!(!spec.argv.is_empty(), "empty entrypoint argv");

        let oci_dir = spec.exec_dir.join("oci");
        fs::create_dir_all(&oci_dir).context("failed to create OCI bundle dir")?;
        fs::write(
            oci_dir.join("config.json"),
            serde_json::to_vec_pretty(&oci_spec(&self.cfg, spec))?,
        )
        .context("failed to write OCI config.json")?;
        fs::create_dir_all(self.state_root(spec))?;

        let mut cmd = Command::new(&self.cfg.runsc_path);
        cmd.arg(format!("--root={}", self.state_root(spec).display()))
            // Reach the per-exec op socket bind-mounted at /run/lit.
            .arg("--host-uds=all")
            // Fresh in-memory tmpfs upper over the shared read-only rootfs:
            // the sandbox can write anywhere, nothing survives it, and the
            // base image is never touched.
            .arg("--overlay2=root:memory")
            .arg(format!("--platform={}", self.cfg.platform))
            .arg(format!("--network={}", self.cfg.network));
        if self.cfg.ignore_cgroups {
            cmd.arg("--ignore-cgroups");
        }
        cmd.arg("run")
            .arg(format!("--bundle={}", oci_dir.display()))
            .arg(&spec.id);
        Ok(cmd)
    }

    fn terminate(&self, spec: &ExecSpec, _pid: Option<u32>) {
        // Kill everything in the sandbox, not just the foreground `runsc run`.
        match self
            .base_cmd(spec)
            .args(["kill", "--all", &spec.id, "KILL"])
            .output()
        {
            Ok(out) if !out.status.success() => {
                warn!(
                    id = spec.id,
                    "runsc kill failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            Err(e) => warn!(id = spec.id, "failed to run runsc kill: {e}"),
            _ => {}
        }
    }

    fn cleanup(&self, spec: &ExecSpec) {
        if let Err(e) = self
            .base_cmd(spec)
            .args(["delete", "-force", &spec.id])
            .output()
        {
            warn!(id = spec.id, "failed to run runsc delete: {e}");
        }
    }
}

/// Minimal OCI runtime spec for one execution.
///
/// The action bundle is bind-mounted read-only at /action (the shared cache
/// copy must never be written); writable scratch space is /tmp, a per-exec
/// size-capped tmpfs. Root writes land in the `--overlay2` memory upper.
fn oci_spec(cfg: &RunscConfig, spec: &ExecSpec) -> serde_json::Value {
    let mut env = vec![
        "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
        format!("{ENV_OP_SOCK}={GUEST_SOCK_DIR}/{OP_SOCK_FILE}"),
        "HOME=/tmp".to_string(),
        "TMPDIR=/tmp".to_string(),
    ];
    env.extend(spec.env.iter().map(|(k, v)| format!("{k}={v}")));

    json!({
        "ociVersion": "1.1.0",
        "hostname": "lit-action",
        "process": {
            "terminal": false,
            // Root inside the sandbox: the gVisor Sentry (plus the CVM) is
            // the isolation boundary, and the base image needn't carry users.
            "user": { "uid": 0, "gid": 0 },
            "args": spec.argv,
            "cwd": GUEST_ACTION_DIR,
            "env": env,
            "rlimits": [
                { "type": "RLIMIT_NOFILE", "hard": 4096, "soft": 4096 }
            ]
        },
        // readonly=false so the overlay upper accepts writes; the underlying
        // rootfs is still never modified (writes go to --overlay2 memory).
        "root": { "path": cfg.rootfs, "readonly": false },
        "mounts": [
            { "destination": "/proc", "type": "proc", "source": "proc" },
            {
                "destination": "/dev",
                "type": "tmpfs",
                "source": "tmpfs",
                "options": ["nosuid", "noexec"]
            },
            {
                "destination": "/tmp",
                "type": "tmpfs",
                "source": "tmpfs",
                "options": ["nosuid", "nodev", format!("size={}m", spec.tmpfs_mb)]
            },
            {
                "destination": GUEST_ACTION_DIR,
                "type": "bind",
                "source": spec.bundle_dir,
                "options": ["bind", "ro"]
            },
            {
                "destination": GUEST_SOCK_DIR,
                "type": "bind",
                "source": spec.sock_dir,
                "options": ["bind", "rw"]
            }
        ],
        "linux": {
            "namespaces": [
                { "type": "pid" },
                { "type": "ipc" },
                { "type": "uts" },
                { "type": "mount" },
                { "type": "network" }
            ],
            "resources": {
                "memory": { "limit": spec.memory_limit_mb * 1024 * 1024 },
                "pids": { "limit": spec.pids_limit }
            },
            // Delegated leaf cgroup per sandbox (container-in-container
            // requirement; see module docs).
            "cgroupsPath": format!("lit-sandboxes/{}", spec.id)
        }
    })
}
