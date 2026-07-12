//! Per-run local state on disk.
//!
//! Every `lit` invocation is a separate process (exactly as inside the
//! sandbox), so anything that must persist across calls within one action
//! run — the master key, recorded response, accumulated logs, fetch counter
//! — lives in a state directory (default `.lit-local/`).

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use rand::RngCore as _;

use crate::hexutil;

const MASTER_KEY_FILE: &str = "master.key";
const RESPONSE_FILE: &str = "response";
const LOGS_FILE: &str = "logs";
const FETCH_COUNT_FILE: &str = "fetch_count";

pub struct State {
    dir: PathBuf,
}

impl State {
    /// Records the dir but does NOT create it: read-only commands
    /// (`job`/`params`/…) leave no `.lit-local/` behind. Writers call
    /// [`Self::ensure_dir`] first.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self { dir: dir.into() })
    }

    fn path(&self, file: &str) -> PathBuf {
        self.dir.join(file)
    }

    /// Create the state dir on first write.
    pub fn ensure_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("failed to create state dir {}", self.dir.display()))
    }

    /// Resolve the 32-byte master key.
    ///
    /// Precedence: explicit `override_hex` (`--key` / env) → persisted
    /// `master.key` → freshly generated (persisted, with a one-time warning
    /// so runs stay reproducible). An explicit key is never written to disk.
    pub fn master_key(&self, override_hex: Option<&str>) -> Result<[u8; 32]> {
        if let Some(hex) = override_hex {
            return parse_key(hex).context("invalid --key / LIT_LOCAL_PRIVATE_KEY");
        }

        let path = self.path(MASTER_KEY_FILE);
        if path.exists() {
            let hex = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            return parse_key(hex.trim()).with_context(|| format!("{} is corrupt", path.display()));
        }

        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        self.ensure_dir()?;
        write_private(
            &path,
            format!("{}\n", hexutil::bytes_to_0x_hex(&key)).as_bytes(),
        )?;
        eprintln!(
            "lit: no master key provided; generated one at {} \
             (set LIT_LOCAL_PRIVATE_KEY to pin it)",
            path.display()
        );
        Ok(key)
    }

    pub fn append_log(&self, message: &str) -> Result<()> {
        self.ensure_dir()?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path(LOGS_FILE))
            .context("failed to open logs file")?;
        f.write_all(message.as_bytes())
            .context("failed to append log")
    }

    pub fn set_response(&self, response: &str) -> Result<()> {
        self.ensure_dir()?;
        std::fs::write(self.path(RESPONSE_FILE), response).context("failed to record response")
    }

    pub fn response(&self) -> Result<Option<String>> {
        match std::fs::read_to_string(self.path(RESPONSE_FILE)) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("failed to read response"),
        }
    }

    /// Increment and persist the fetch counter, returning the new value.
    pub fn increment_fetch_count(&self) -> Result<u32> {
        let path = self.path(FETCH_COUNT_FILE);
        let current: u32 = match std::fs::read_to_string(&path) {
            Ok(s) => s.trim().parse().unwrap_or(0),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
            Err(e) => return Err(e).context("failed to read fetch count"),
        };
        let next = current + 1;
        self.ensure_dir()?;
        std::fs::write(&path, next.to_string()).context("failed to write fetch count")?;
        Ok(next)
    }

    /// Clear mutable per-run state (response, logs, counter) but keep the
    /// master key so derived values stay stable across runs.
    pub fn reset_run(&self) -> Result<()> {
        for file in [RESPONSE_FILE, LOGS_FILE, FETCH_COUNT_FILE] {
            match std::fs::remove_file(self.path(file)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e).context("failed to reset run state"),
            }
        }
        Ok(())
    }
}

fn parse_key(hex: &str) -> Result<[u8; 32]> {
    let bytes = hexutil::hex_to_bytes(hex)?;
    match <[u8; 32]>::try_from(bytes.as_slice()) {
        Ok(key) => Ok(key),
        Err(_) => bail!("expected a 32-byte key, got {} bytes", bytes.len()),
    }
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    f.write_all(bytes).context("failed to write key")
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).with_context(|| format!("failed to create {}", path.display()))
}
