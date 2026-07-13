//! Direct-subprocess runtime: NO isolation.
//!
//! For integration tests and local development on hosts without gVisor
//! (e.g. macOS). Presents the same guest contract as the runsc runtime —
//! `LIT_OP_SOCK`, the `lit` CLI on PATH, cwd inside the action dir — so a
//! bundle developed against it runs unchanged in production.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, ensure};
use tokio::process::Command;
use tracing::warn;

use super::{ENV_OP_SOCK, ExecSpec, OP_SOCK_FILE, SandboxRuntime};

pub struct ProcessRuntime {
    /// Directory containing the guest `lit` CLI; prepended to PATH.
    pub guest_bin_dir: PathBuf,
}

impl SandboxRuntime for ProcessRuntime {
    fn name(&self) -> &'static str {
        "process"
    }

    fn command(&self, spec: &ExecSpec) -> Result<Command> {
        ensure!(!spec.argv.is_empty(), "empty entrypoint argv");

        // Copy the bundle into a per-exec dir so the child never runs inside
        // the shared read-only cache (parity with the runsc mount: the cache
        // copy must survive a badly-behaved action).
        let action_dir = spec.exec_dir.join("action");
        copy_dir(&spec.bundle_dir, &action_dir).context("failed to stage action dir")?;

        let mut cmd = Command::new(&spec.argv[0]);
        cmd.args(&spec.argv[1..]);
        cmd.current_dir(&action_dir);
        cmd.env(ENV_OP_SOCK, spec.sock_dir.join(OP_SOCK_FILE));
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        let path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{path}", self.guest_bin_dir.display()));
        // Own process group so terminate() can kill the whole tree (shells
        // spawn children; killing only the direct child would orphan them).
        cmd.process_group(0);
        Ok(cmd)
    }

    fn terminate(&self, _spec: &ExecSpec, pid: Option<u32>) {
        if let Some(pid) = pid {
            let pgid = nix::unistd::Pid::from_raw(pid as i32);
            if let Err(e) = nix::sys::signal::killpg(pgid, nix::sys::signal::Signal::SIGKILL)
                && e != nix::errno::Errno::ESRCH
            {
                warn!("failed to kill process group {pid}: {e}");
            }
        }
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)?;
        } else {
            // Bundle unpack rejects link/device/FIFO tar entries, so nothing
            // else can exist here. Fail loudly rather than stage a bundle
            // that would behave differently under the runsc runtime.
            anyhow::bail!(
                "unsupported file type in bundle: {}",
                entry.path().display()
            );
        }
    }
    Ok(())
}
