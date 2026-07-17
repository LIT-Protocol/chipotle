//! Build an any-language Lit Action bundle from a target folder.
//!
//! The bundle format matches what the gVisor runner unpacks
//! (`lit-actions/gvisor-server/src/bundle.rs`): a `tar`/`tar.gz` archive whose
//! root carries the files the action needs plus a `startup.sh` the sandbox runs
//! as `bash startup.sh`. An optional `lit.json` manifest holds metadata only
//! (`runtime`, `env`); a legacy `entrypoint` field is ignored by the runner.
//!
//! The bundle is content-addressed: its checksum (an IPFS CID) is
//! `IpfsHasher::default().compute(<bundle bytes>)`, the exact derivation
//! lit-api-server performs in `resolve_binary_bundle`, so the CID printed here
//! equals the one the server authorizes and caches under.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};

/// The manifest file name every bundle may carry at its root.
pub const MANIFEST_FILE: &str = "lit.json";
/// The entrypoint the sandbox always executes (`bash startup.sh`).
pub const STARTUP_FILE: &str = "startup.sh";

/// Caps mirroring the runner so a bundle that would be rejected on unpack is
/// caught locally instead.
const MAX_UNPACKED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;

/// Inputs for a bundle build.
pub struct BundleSpec {
    /// The target folder to package.
    pub dir: PathBuf,
    /// Optional `lit.json` to include (overrides any `lit.json` in `dir`).
    pub config: Option<PathBuf>,
    /// Binary to run: when the folder has no `startup.sh`, a `startup.sh`
    /// launching this file is generated.
    pub binary: Option<String>,
    /// gzip the tar (default) or emit a plain tar.
    pub compress: bool,
}

/// A built bundle: the raw archive bytes and their content id.
pub struct BuiltBundle {
    pub bytes: Vec<u8>,
    pub checksum: String,
    /// True when a `startup.sh` was synthesized from `--binary`.
    pub generated_startup: bool,
}

/// One file destined for the archive, already resolved to its bytes + mode.
struct Entry {
    /// Path relative to the archive root, forward-slashed.
    path: String,
    contents: Vec<u8>,
    /// Unix mode; only the rwx bits are meaningful to the runner.
    mode: u32,
}

/// Build the bundle described by `spec`.
pub fn build(spec: &BundleSpec) -> Result<BuiltBundle> {
    ensure!(
        spec.dir.is_dir(),
        "target `{}` is not a directory",
        spec.dir.display()
    );

    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    collect_dir(&spec.dir, &spec.dir, &mut entries)?;

    // A supplied --config replaces any lit.json already in the folder.
    if let Some(config) = &spec.config {
        let contents = std::fs::read(config)
            .with_context(|| format!("failed to read config `{}`", config.display()))?;
        // Fail fast on invalid JSON rather than shipping a manifest the runner
        // would reject.
        serde_json::from_slice::<serde_json::Value>(&contents)
            .with_context(|| format!("config `{}` is not valid JSON", config.display()))?;
        insert(
            &mut entries,
            Entry {
                path: MANIFEST_FILE.to_string(),
                contents,
                mode: 0o644,
            },
        );
    }

    // Resolve the entrypoint. A folder startup.sh always wins; otherwise we
    // synthesize one from --binary. With neither, there is nothing to run.
    let mut generated_startup = false;
    if !entries.contains_key(STARTUP_FILE) {
        let binary = spec.binary.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "`{dir}` has no {STARTUP_FILE} and no config describing how to run it; \
                 pass --binary <file> to generate one, or add a {STARTUP_FILE}",
                dir = spec.dir.display(),
            )
        })?;
        ensure!(
            entries.contains_key(binary),
            "--binary `{binary}` is not a file inside `{}`",
            spec.dir.display()
        );
        // The generated launcher execs the binary; the runner injects
        // top-level js_params as environment variables before this runs.
        insert(
            &mut entries,
            Entry {
                path: STARTUP_FILE.to_string(),
                contents: generated_startup_sh(binary).into_bytes(),
                mode: 0o755,
            },
        );
        // A binary needs its exec bit set in the sandbox's read-only mount.
        if let Some(bin) = entries.get_mut(binary) {
            bin.mode |= 0o755;
        }
        generated_startup = true;
    }

    ensure!(
        entries.len() <= MAX_ENTRIES,
        "bundle exceeds {MAX_ENTRIES} entries"
    );
    let total: u64 = entries.values().map(|e| e.contents.len() as u64).sum();
    ensure!(
        total <= MAX_UNPACKED_BYTES,
        "bundle exceeds {MAX_UNPACKED_BYTES} unpacked bytes"
    );

    let bytes = pack(&entries, spec.compress)?;
    let checksum = ipfs_hasher::IpfsHasher::default().compute(&bytes);

    Ok(BuiltBundle {
        bytes,
        checksum,
        generated_startup,
    })
}

/// Compute the content id for already-built bundle bytes (e.g. a prebuilt
/// archive on disk), matching the server's derivation.
pub fn checksum(bytes: &[u8]) -> String {
    ipfs_hasher::IpfsHasher::default().compute(bytes)
}

/// Recursively gather regular files under `root`, keyed by their archive-root
/// relative path. Symlinks and other special files are skipped — the runner
/// rejects them anyway, so shipping them would only produce an unpackable
/// bundle.
fn collect_dir(root: &Path, dir: &Path, out: &mut BTreeMap<String, Entry>) -> Result<()> {
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory `{}`", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_dir(root, &path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .expect("path is under root")
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let contents = std::fs::read(&path)
            .with_context(|| format!("failed to read file `{}`", path.display()))?;
        insert(
            out,
            Entry {
                path: rel,
                contents,
                mode: mode_of(&path),
            },
        );
    }
    Ok(())
}

fn insert(out: &mut BTreeMap<String, Entry>, entry: Entry) {
    out.insert(entry.path.clone(), entry);
}

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o777)
        .unwrap_or(0o644)
}

#[cfg(not(unix))]
fn mode_of(_path: &Path) -> u32 {
    // No Unix mode off-Unix; default to a sane readable mode. Exec bits for a
    // --binary target are applied explicitly by the caller.
    0o644
}

fn generated_startup_sh(binary: &str) -> String {
    format!(
        "#!/usr/bin/env bash\n\
         # Generated by lit-bundle. The sandbox runs `bash startup.sh`; top-level\n\
         # js_params arrive as environment variables. Exec the packaged binary.\n\
         set -euo pipefail\n\
         exec ./{binary}\n"
    )
}

/// Serialize `entries` into a deterministic tar (optionally gzip'd): sorted
/// paths (the BTreeMap iteration order), zeroed mtime/uid/gid, so identical
/// inputs yield identical bytes — hence an identical CID and a runner cache
/// hit on repeat deploys.
fn pack(entries: &BTreeMap<String, Entry>, compress: bool) -> Result<Vec<u8>> {
    let mut tar_bytes = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        for entry in entries.values() {
            let mut header = tar::Header::new_gnu();
            header.set_size(entry.contents.len() as u64);
            header.set_mode(entry.mode);
            header.set_mtime(0);
            header.set_uid(0);
            header.set_gid(0);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            builder
                .append_data(&mut header, &entry.path, entry.contents.as_slice())
                .with_context(|| format!("failed to add `{}` to bundle", entry.path))?;
        }
        builder.finish().context("failed to finalize tar")?;
    }

    if !compress {
        return Ok(tar_bytes);
    }

    // Pin the gzip header (mtime 0, fixed OS byte) so the compressed output is
    // reproducible too.
    let mut gz = flate2::GzBuilder::new()
        .mtime(0)
        .operating_system(255) // "unknown", stable across platforms
        .write(Vec::new(), flate2::Compression::default());
    gz.write_all(&tar_bytes).context("failed to gzip bundle")?;
    gz.finish().context("failed to finish gzip stream")
}

/// Guard against a `--binary` argument containing path separators or traversal;
/// it must name a file at the bundle root.
pub fn validate_binary_name(name: &str) -> Result<()> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == ".." {
        bail!("--binary must be a plain file name at the bundle root, got `{name}`");
    }
    Ok(())
}
