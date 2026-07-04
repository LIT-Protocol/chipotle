//! Content-addressed action bundles.
//!
//! The any-language analog of the JS runner's `ActionCodeCache`: the
//! `ExecutionRequest.code` field carries either a base64 tar(.gz) bundle
//! (hashed to a content id and unpacked once) or a `cid:<id>` reference to a
//! previously-unpacked bundle. The unpacked directory is treated as
//! read-only and shared across executions; per-run writes land in the
//! sandbox's tmpfs, never here.
//!
//! Permissions and key derivation key on the bundle's IPFS CID exactly like
//! JS actions do; lit-api-server derives that CID from the bundle bytes and
//! passes it as `ipfs_id`. This cache only needs a stable key, so when
//! `ipfs_id` is absent (tests, direct calls) it falls back to a local
//! `sha256-<hex>` pseudo-id.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail, ensure};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest as _, Sha256};

/// Manifest file every bundle must carry at its root.
pub const MANIFEST_FILE: &str = "lit.json";
/// `code` prefix referencing an already-cached bundle by content id.
pub const CID_REF_PREFIX: &str = "cid:";

/// Decompression caps so a hostile bundle can't zip-bomb the runner.
const MAX_UNPACKED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Manifest {
    /// What to run. A string is a shell script path (run as `sh <path>`, no
    /// exec bit needed); an array is an argv executed verbatim.
    pub entrypoint: Entrypoint,
    /// Informational runtime the bundle targets (e.g. "python3"). The v1
    /// base image ships all supported runtimes, so this is not yet enforced.
    #[serde(default)]
    pub runtime: Option<String>,
    /// Extra environment variables for the entrypoint.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum Entrypoint {
    Argv(Vec<String>),
    Script(String),
}

impl Entrypoint {
    pub fn to_argv(&self) -> Result<Vec<String>> {
        let argv = match self {
            Entrypoint::Argv(argv) => argv.clone(),
            Entrypoint::Script(path) => vec!["/bin/sh".to_string(), path.clone()],
        };
        ensure!(
            !argv.is_empty() && !argv[0].trim().is_empty(),
            "manifest entrypoint must not be empty"
        );
        Ok(argv)
    }
}

#[derive(Debug)]
pub struct Bundle {
    /// Content id: the caller-supplied IPFS CID, or `sha256-<hex>`.
    pub cid: String,
    /// Read-only unpacked bundle contents, shared across executions.
    pub dir: PathBuf,
    pub manifest: Manifest,
}

#[derive(Debug, Clone)]
pub struct BundleCache {
    root: PathBuf,
}

impl BundleCache {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)
            .with_context(|| format!("failed to create bundle cache dir {}", root.display()))?;
        Ok(Self { root })
    }

    /// Resolve `code` (+ optional server-derived `ipfs_id`) to an unpacked
    /// bundle. Blocking (tar/gzip file IO) — call via `spawn_blocking`.
    pub fn resolve(&self, code: &str, ipfs_id: Option<&str>) -> Result<Bundle> {
        let code = code.trim();

        if let Some(cid) = code.strip_prefix(CID_REF_PREFIX) {
            let cid = validate_cid(cid)?;
            let dir = self.root.join(&cid);
            if !dir.is_dir() {
                bail!("bundle {cid} is not cached on this runner; resend the bundle bytes");
            }
            return load(cid, dir);
        }

        // Base64 payload; tolerate embedded whitespace/newlines.
        let compact: String = code.split_whitespace().collect();
        let bytes = BASE64
            .decode(compact.as_bytes())
            .context("code is neither a `cid:<id>` reference nor a valid base64-encoded bundle")?;

        let cid = match ipfs_id.map(str::trim) {
            Some(id) if !id.is_empty() => validate_cid(id)?,
            _ => format!("sha256-{}", hex(&Sha256::digest(&bytes))),
        };

        let dir = self.root.join(&cid);
        if !dir.is_dir() {
            self.unpack(&bytes, &cid)?;
        }
        load(cid, dir)
    }

    /// Unpack into a temp sibling then rename, so a concurrent resolve of the
    /// same CID either wins the rename or finds the winner's directory —
    /// never a half-unpacked one.
    fn unpack(&self, bytes: &[u8], cid: &str) -> Result<()> {
        let tmp = tempfile::tempdir_in(&self.root).context("failed to create unpack dir")?;

        let reader: Box<dyn Read + '_> = if bytes.starts_with(&GZIP_MAGIC) {
            Box::new(flate2::read::GzDecoder::new(bytes))
        } else {
            Box::new(bytes)
        };
        let mut archive = tar::Archive::new(reader);
        archive.set_preserve_permissions(true);

        let mut total: u64 = 0;
        let mut entries: usize = 0;
        for entry in archive.entries().context("bundle is not a tar archive")? {
            let mut entry = entry.context("failed to read bundle tar entry")?;
            entries += 1;
            ensure!(
                entries <= MAX_ENTRIES,
                "bundle exceeds {MAX_ENTRIES} entries"
            );
            total = total.saturating_add(entry.size());
            ensure!(
                total <= MAX_UNPACKED_BYTES,
                "bundle exceeds {MAX_UNPACKED_BYTES} unpacked bytes"
            );
            // unpack_in refuses paths escaping the target dir (`..`, absolute).
            entry
                .unpack_in(tmp.path())
                .context("failed to unpack bundle tar entry")?;
        }

        // Validate the manifest before publishing so a broken bundle is never
        // cached under its CID.
        read_manifest(tmp.path())?;

        let dir = self.root.join(cid);
        let tmp = tmp.keep();
        if let Err(e) = fs::rename(&tmp, &dir) {
            let _ = fs::remove_dir_all(&tmp);
            // Lost a race against a concurrent unpack of the same CID.
            if !dir.is_dir() {
                return Err(e).context("failed to publish unpacked bundle");
            }
        }
        Ok(())
    }
}

fn load(cid: String, dir: PathBuf) -> Result<Bundle> {
    let manifest = read_manifest(&dir)?;
    Ok(Bundle { cid, dir, manifest })
}

fn read_manifest(dir: &std::path::Path) -> Result<Manifest> {
    let path = dir.join(MANIFEST_FILE);
    let raw = fs::read(&path)
        .with_context(|| format!("bundle is missing its {MANIFEST_FILE} manifest"))?;
    let manifest: Manifest = serde_json::from_slice(&raw)
        .with_context(|| format!("bundle {MANIFEST_FILE} manifest is invalid"))?;
    // Fail fast on an empty entrypoint at unpack time, not spawn time.
    manifest.entrypoint.to_argv()?;
    Ok(manifest)
}

/// The CID doubles as a cache directory name, so restrict it to filename-safe
/// characters (real CIDs are base32/base58 alphanumerics).
fn validate_cid(cid: &str) -> Result<String> {
    ensure!(
        !cid.is_empty()
            && cid.len() <= 128
            && cid
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid bundle content id: {cid:?}"
    );
    Ok(cid.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn tar_gz(files: &[(&str, &str)]) -> String {
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            for (name, contents) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(contents.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, name, contents.as_bytes())
                    .unwrap();
            }
            builder.finish().unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar_bytes).unwrap();
        BASE64.encode(gz.finish().unwrap())
    }

    #[test]
    fn resolve_unpacks_and_caches_by_sha() {
        let root = tempfile::tempdir().unwrap();
        let cache = BundleCache::new(root.path()).unwrap();
        let code = tar_gz(&[
            ("lit.json", r#"{"entrypoint": ["/bin/sh", "run.sh"]}"#),
            ("run.sh", "echo hi\n"),
        ]);

        let bundle = cache.resolve(&code, None).unwrap();
        assert!(bundle.cid.starts_with("sha256-"));
        assert!(bundle.dir.join("run.sh").is_file());
        assert_eq!(bundle.manifest.entrypoint.to_argv().unwrap()[0], "/bin/sh");

        // Second resolve via cid: reference hits the cache.
        let via_ref = cache.resolve(&format!("cid:{}", bundle.cid), None).unwrap();
        assert_eq!(via_ref.dir, bundle.dir);
    }

    #[test]
    fn resolve_prefers_server_supplied_ipfs_id() {
        let root = tempfile::tempdir().unwrap();
        let cache = BundleCache::new(root.path()).unwrap();
        let code = tar_gz(&[("lit.json", r#"{"entrypoint": "run.sh"}"#), ("run.sh", "")]);
        let bundle = cache.resolve(&code, Some("QmTestCid123")).unwrap();
        assert_eq!(bundle.cid, "QmTestCid123");
    }

    #[test]
    fn uncached_cid_reference_errors() {
        let root = tempfile::tempdir().unwrap();
        let cache = BundleCache::new(root.path()).unwrap();
        let err = cache.resolve("cid:QmMissing", None).unwrap_err();
        assert!(err.to_string().contains("not cached"), "{err:#}");
    }

    #[test]
    fn garbage_code_errors() {
        let root = tempfile::tempdir().unwrap();
        let cache = BundleCache::new(root.path()).unwrap();
        let err = cache.resolve("!!! not a bundle !!!", None).unwrap_err();
        assert!(err.to_string().contains("base64"), "{err:#}");
    }

    #[test]
    fn missing_manifest_errors_and_is_not_cached() {
        let root = tempfile::tempdir().unwrap();
        let cache = BundleCache::new(root.path()).unwrap();
        let code = tar_gz(&[("run.sh", "echo hi\n")]);
        let err = cache.resolve(&code, Some("QmNoManifest")).unwrap_err();
        assert!(err.to_string().contains(MANIFEST_FILE), "{err:#}");
        // The broken bundle must not have been published under its CID.
        assert!(cache.resolve("cid:QmNoManifest", None).is_err());
    }

    #[test]
    fn traversal_cid_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let cache = BundleCache::new(root.path()).unwrap();
        assert!(cache.resolve("cid:../../etc", None).is_err());
    }
}
