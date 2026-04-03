use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use base64::Engine;
use deno_core::{
    ModuleLoadResponse, ModuleLoader, ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType,
    RequestedModuleType, ResolutionKind,
};
use deno_error::JsErrorBox;
use futures::FutureExt;
use sha2::{Digest, Sha384};
use tracing::{debug, error, info, warn};

/// Constant-time byte comparison to prevent timing side-channels on integrity hashes.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Allowed CDN URL prefix for module imports.
const ALLOWED_CDN_PREFIX: &str = "https://cdn.jsdelivr.net/";

/// Maximum response body size (10 MB).
const MAX_MODULE_SIZE_BYTES: usize = 10 * 1024 * 1024;

/// HTTP request timeout.
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// A module loader that only allows imports from approved CDN URLs
/// and verifies each module's SHA-384 integrity hash against a manifest.
/// Thread-safe cache for fetched and integrity-verified module sources.
pub type ModuleCache = Arc<RwLock<HashMap<String, Vec<u8>>>>;

pub struct CdnModuleLoader {
    /// Maps CDN URLs to their expected base64-encoded SHA-384 hashes.
    integrity: Arc<RwLock<HashMap<String, String>>>,
    /// If true (production), reject any module not present in the integrity manifest.
    strict: bool,
    /// Reusable HTTP client with timeouts and redirect policy.
    client: reqwest::Client,
    /// Cache of fetched module sources, shared across loader instances.
    cache: ModuleCache,
    /// Path to integrity.lock file on disk. When set, enables trust-on-first-use:
    /// new modules are double-fetched, verified, and pinned to the lockfile.
    lockfile_path: Option<PathBuf>,
}

impl CdnModuleLoader {
    /// Create a new `CdnModuleLoader`.
    ///
    /// - `integrity`: parsed integrity manifest mapping URL → base64-encoded SHA-384 hash
    /// - `strict`: if true, modules not present in the manifest are rejected (unless lockfile_path is set for TOFU)
    pub fn new(integrity: Arc<RwLock<HashMap<String, String>>>, strict: bool) -> Self {
        Self::with_options(integrity, strict, Arc::new(RwLock::new(HashMap::new())), None)
    }

    /// Create a new `CdnModuleLoader` with a shared cache and optional lockfile path.
    pub fn with_options(
        integrity: Arc<RwLock<HashMap<String, String>>>,
        strict: bool,
        cache: ModuleCache,
        lockfile_path: Option<PathBuf>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build HTTP client");

        Self {
            integrity,
            strict,
            client,
            cache,
            lockfile_path,
        }
    }

    /// Parse an `integrity.lock` file into a URL → hash map.
    ///
    /// Format: one entry per line, `<url> sha384-<base64hash>`
    /// Empty lines and lines starting with `#` are ignored.
    pub fn parse_integrity_lock(contents: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for (line_no, line) in contents.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Expected format: <url> sha384-<base64hash>
            if let Some((url, hash)) = line.split_once(' ') {
                let hash = hash.trim();
                if let Some(digest) = hash.strip_prefix("sha384-") {
                    if map.contains_key(url) {
                        warn!(
                            "integrity.lock:{}: duplicate entry for URL {url}, overwriting",
                            line_no + 1
                        );
                    }
                    map.insert(url.to_string(), digest.to_string());
                } else {
                    warn!(
                        "integrity.lock:{}: unsupported hash algorithm (expected sha384-), skipping: {line}",
                        line_no + 1
                    );
                }
            } else {
                warn!(
                    "integrity.lock:{}: malformed line (expected '<url> sha384-<hash>'), skipping: {line}",
                    line_no + 1
                );
            }
        }
        map
    }

    /// Check whether a URL is from the allowed CDN.
    fn is_allowed_cdn(url: &str) -> bool {
        url.starts_with(ALLOWED_CDN_PREFIX)
    }
}

impl ModuleLoader for CdnModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        _referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, deno_core::error::ModuleLoaderError> {
        // Only allow full CDN URLs — no bare specifiers, no relative imports.
        if !Self::is_allowed_cdn(specifier) {
            warn!(
                specifier,
                referrer = _referrer,
                "CDN module resolve rejected: specifier is not from allowed CDN"
            );
            return Err(JsErrorBox::generic(format!(
                "Bare specifiers are not allowed: \"{specifier}\". \
                 Use the full CDN URL (e.g. https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm)"
            ))
            .into());
        }

        ModuleSpecifier::parse(specifier)
            .map_err(|e| JsErrorBox::generic(format!("Invalid module URL: {specifier}: {e}")).into())
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        _maybe_referrer: Option<&ModuleSpecifier>,
        _is_dyn_import: bool,
        _requested_module_type: RequestedModuleType,
    ) -> ModuleLoadResponse {
        let url = module_specifier.to_string();
        let strict = self.strict;
        let expected_hash = self
            .integrity
            .read()
            .expect("integrity lock poisoned")
            .get(&url)
            .cloned();

        // In strict mode without a lockfile path, reject unknown modules.
        // With a lockfile path, we allow TOFU (trust-on-first-use) instead.
        if strict && expected_hash.is_none() && self.lockfile_path.is_none() {
            error!(
                module_url = %url,
                "CDN module rejected: not in integrity manifest and strict mode is enabled without TOFU lockfile"
            );
            return ModuleLoadResponse::Sync(Err(JsErrorBox::generic(format!(
                "Module {url} is not present in the integrity manifest. \
                 In strict mode, all modules must have an integrity entry."
            ))
            .into()));
        }

        // Check cache first
        if let Ok(cache) = self.cache.read() {
            if let Some(cached_bytes) = cache.get(&url) {
                debug!(module_url = %url, size_bytes = cached_bytes.len(), "CDN module loaded from cache");
                return ModuleLoadResponse::Sync(Ok(ModuleSource::new(
                    ModuleType::JavaScript,
                    ModuleSourceCode::Bytes(cached_bytes.clone().into_boxed_slice().into()),
                    module_specifier,
                    None,
                )));
            }
        }

        let client = self.client.clone();
        let cache = self.cache.clone();
        let integrity = self.integrity.clone();
        let lockfile_path = self.lockfile_path.clone();

        let fut = async move {
            info!(module_url = %url, "CDN module fetch: downloading");

            // Fetch the module source from the CDN (redirects disabled)
            let response = client.get(&url).send().await.map_err(|e| {
                error!(module_url = %url, error = %e, "CDN module fetch failed");
                JsErrorBox::generic(format!("Failed to fetch module {url}: {e}"))
            })?;

            let status = response.status();
            if status.is_redirection() {
                error!(module_url = %url, http_status = %status, "CDN module fetch rejected: redirect");
                return Err(JsErrorBox::generic(format!(
                    "Module {url} returned a redirect (HTTP {status}). \
                     Redirects are not allowed for CDN module imports."
                ))
                .into());
            }

            if !status.is_success() {
                error!(module_url = %url, http_status = %status, "CDN module fetch failed: non-success status");
                return Err(JsErrorBox::generic(format!(
                    "Failed to fetch module {url}: HTTP {status}"
                ))
                .into());
            }

            // Extract SRI hash from CDN response headers (jsDelivr provides this)
            let cdn_sri_hash = response
                .headers()
                .get("x-sri-hash")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("sha384-"))
                .map(|s| s.to_string());

            if let Some(ref sri) = cdn_sri_hash {
                debug!(module_url = %url, cdn_sri = %format!("sha384-{sri}"), "CDN provided SRI hash in response header");
            }

            // Enforce response size limit
            if let Some(len) = response.content_length() {
                if len as usize > MAX_MODULE_SIZE_BYTES {
                    error!(module_url = %url, content_length = len, max_bytes = MAX_MODULE_SIZE_BYTES, "CDN module rejected: exceeds size limit");
                    return Err(JsErrorBox::generic(format!(
                        "Module {url} exceeds maximum size ({len} bytes > {MAX_MODULE_SIZE_BYTES} bytes)"
                    ))
                    .into());
                }
            }

            let bytes = response.bytes().await.map_err(|e| {
                error!(module_url = %url, error = %e, "CDN module fetch: failed to read response body");
                JsErrorBox::generic(format!(
                    "Failed to read response body for {url}: {e}"
                ))
            })?;

            if bytes.len() > MAX_MODULE_SIZE_BYTES {
                error!(module_url = %url, body_size = bytes.len(), max_bytes = MAX_MODULE_SIZE_BYTES, "CDN module rejected: body exceeds size limit");
                return Err(JsErrorBox::generic(format!(
                    "Module {url} exceeds maximum size ({} bytes > {MAX_MODULE_SIZE_BYTES} bytes)",
                    bytes.len()
                ))
                .into());
            }

            info!(module_url = %url, size_bytes = bytes.len(), "CDN module fetch: download complete");

            let mut hasher = Sha384::new();
            hasher.update(&bytes);
            let actual_digest = hasher.finalize();

            let actual_b64 = base64::engine::general_purpose::STANDARD
                .encode(&actual_digest);

            if let Some(expected_b64) = &expected_hash {
                // Known module: verify against stored hash
                let expected_digest = base64::engine::general_purpose::STANDARD
                    .decode(expected_b64)
                    .map_err(|e| {
                        error!(module_url = %url, error = %e, "CDN module integrity: invalid base64 in manifest");
                        JsErrorBox::generic(format!(
                            "Invalid base64 in integrity manifest for {url}: {e}"
                        ))
                    })?;

                // Constant-time comparison to prevent timing side-channels
                if actual_digest.len() != expected_digest.len()
                    || !constant_time_eq(&actual_digest, &expected_digest)
                {
                    error!(
                        module_url = %url,
                        expected_hash = %format!("sha384-{expected_b64}"),
                        actual_hash = %format!("sha384-{actual_b64}"),
                        "CDN module integrity check FAILED: hash mismatch"
                    );
                    return Err(JsErrorBox::generic(format!(
                        "Integrity check failed for {url}: \
                         expected sha384-{expected_b64}, got sha384-{actual_b64}"
                    ))
                    .into());
                }
                info!(
                    module_url = %url,
                    hash = %format!("sha384-{actual_b64}"),
                    size_bytes = bytes.len(),
                    "CDN module integrity check passed"
                );
            } else {
                // Unknown module: trust-on-first-use (TOFU)
                // Fetch the module a second time and verify the hash matches.
                info!(
                    module_url = %url,
                    first_hash = %format!("sha384-{actual_b64}"),
                    "TOFU: new module detected, starting verification fetch"
                );

                let response2 = client.get(&url).send().await.map_err(|e| {
                    error!(module_url = %url, error = %e, "TOFU: verification fetch failed");
                    JsErrorBox::generic(format!(
                        "TOFU verification fetch failed for {url}: {e}"
                    ))
                })?;

                let status2 = response2.status();
                if !status2.is_success() {
                    error!(module_url = %url, http_status = %status2, "TOFU: verification fetch returned non-success status");
                    return Err(JsErrorBox::generic(format!(
                        "TOFU verification fetch failed for {url}: HTTP {status2}"
                    ))
                    .into());
                }

                let bytes2 = response2.bytes().await.map_err(|e| {
                    error!(module_url = %url, error = %e, "TOFU: verification fetch failed to read body");
                    JsErrorBox::generic(format!(
                        "TOFU verification: failed to read response body for {url}: {e}"
                    ))
                })?;

                let mut hasher2 = Sha384::new();
                hasher2.update(&bytes2);
                let verify_digest = hasher2.finalize();
                let verify_b64 = base64::engine::general_purpose::STANDARD
                    .encode(&verify_digest);

                if !constant_time_eq(&actual_digest, &verify_digest) {
                    error!(
                        module_url = %url,
                        first_hash = %format!("sha384-{actual_b64}"),
                        second_hash = %format!("sha384-{verify_b64}"),
                        first_size = bytes.len(),
                        second_size = bytes2.len(),
                        "TOFU: REJECTED — CDN returned inconsistent content between fetches. Possible tampering."
                    );
                    return Err(JsErrorBox::generic(format!(
                        "TOFU: CDN returned inconsistent content for {url}. \
                         Hash mismatch between first and second fetch. Possible tampering."
                    ))
                    .into());
                }

                info!(
                    module_url = %url,
                    hash = %format!("sha384-{actual_b64}"),
                    size_bytes = bytes.len(),
                    "TOFU: verification passed — both fetches produced identical hash"
                );

                // Verify against CDN's SRI hash header if available (three-way check)
                if let Some(ref sri_b64) = cdn_sri_hash {
                    let sri_digest = base64::engine::general_purpose::STANDARD
                        .decode(sri_b64)
                        .map_err(|e| {
                            error!(module_url = %url, cdn_sri = %format!("sha384-{sri_b64}"), error = %e, "TOFU: CDN SRI header contains invalid base64");
                            JsErrorBox::generic(format!(
                                "CDN SRI header for {url} contains invalid base64: {e}"
                            ))
                        })?;

                    if !constant_time_eq(&actual_digest, &sri_digest) {
                        error!(
                            module_url = %url,
                            computed_hash = %format!("sha384-{actual_b64}"),
                            cdn_sri = %format!("sha384-{sri_b64}"),
                            "TOFU: REJECTED — computed hash does not match CDN SRI header. Possible tampering."
                        );
                        return Err(JsErrorBox::generic(format!(
                            "TOFU: computed hash does not match CDN SRI header for {url}. \
                             Computed sha384-{actual_b64}, CDN declared sha384-{sri_b64}."
                        ))
                        .into());
                    }
                    info!(
                        module_url = %url,
                        hash = %format!("sha384-{actual_b64}"),
                        "TOFU: three-way verification passed (first fetch, second fetch, CDN SRI header)"
                    );
                }

                // Pin to lockfile on disk
                if let Some(ref path) = lockfile_path {
                    use std::io::Write;
                    let mut file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .map_err(|e| {
                            error!(module_url = %url, lockfile = ?path, error = %e, "TOFU: failed to open integrity lockfile");
                            JsErrorBox::generic(format!(
                                "Failed to open integrity lockfile: {e}"
                            ))
                        })?;
                    writeln!(file, "{url} sha384-{actual_b64}").map_err(|e| {
                        error!(module_url = %url, lockfile = ?path, error = %e, "TOFU: failed to write integrity lockfile");
                        JsErrorBox::generic(format!(
                            "Failed to write integrity lockfile: {e}"
                        ))
                    })?;
                    file.flush().map_err(|e| {
                        error!(module_url = %url, lockfile = ?path, error = %e, "TOFU: failed to flush integrity lockfile");
                        JsErrorBox::generic(format!(
                            "Failed to flush integrity lockfile: {e}"
                        ))
                    })?;
                    info!(
                        module_url = %url,
                        hash = %format!("sha384-{actual_b64}"),
                        lockfile = ?path,
                        "TOFU: pinned new module to integrity lockfile"
                    );
                }

                // Update in-memory manifest (re-check for concurrent pin)
                if let Ok(mut map) = integrity.write() {
                    map.entry(url.clone()).or_insert(actual_b64.clone());
                }
            }

            let bytes_vec = bytes.to_vec();

            // Cache the verified module source
            if let Ok(mut cache_w) = cache.write() {
                cache_w.insert(url.clone(), bytes_vec.clone());
                debug!(module_url = %url, "CDN module cached for future requests");
            }

            info!(module_url = %url, size_bytes = bytes_vec.len(), "CDN module loaded successfully");

            Ok(ModuleSource::new(
                ModuleType::JavaScript,
                ModuleSourceCode::Bytes(bytes_vec.into_boxed_slice().into()),
                &ModuleSpecifier::parse(&url).unwrap(),
                None,
            ))
        }
        .boxed_local();

        ModuleLoadResponse::Async(fut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_integrity_lock() {
        let contents = r#"
# This is a comment
https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm sha384-abc123def456
https://cdn.jsdelivr.net/npm/lodash-es@4.17.21/+esm sha384-xyz789

# Another comment
"#;
        let map = CdnModuleLoader::parse_integrity_lock(contents);
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get("https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm"),
            Some(&"abc123def456".to_string())
        );
        assert_eq!(
            map.get("https://cdn.jsdelivr.net/npm/lodash-es@4.17.21/+esm"),
            Some(&"xyz789".to_string())
        );
    }

    #[test]
    fn test_is_allowed_cdn() {
        assert!(CdnModuleLoader::is_allowed_cdn(
            "https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm"
        ));
        assert!(!CdnModuleLoader::is_allowed_cdn("https://esm.sh/lodash-es@4.17.21"));
        assert!(!CdnModuleLoader::is_allowed_cdn("https://evil.com/malware.js"));
        assert!(!CdnModuleLoader::is_allowed_cdn("lodash-es"));
        assert!(!CdnModuleLoader::is_allowed_cdn("./local.js"));
    }

    #[test]
    fn test_resolve_rejects_bare_specifiers() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        assert!(loader
            .resolve("lodash-es", "file:///main.js", ResolutionKind::Import)
            .is_err());
    }

    #[test]
    fn test_resolve_allows_cdn_urls() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        assert!(loader
            .resolve(
                "https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm",
                "file:///main.js",
                ResolutionKind::Import,
            )
            .is_ok());
    }

    #[test]
    fn test_strict_mode_rejects_unknown_modules() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), true);
        let specifier =
            ModuleSpecifier::parse("https://cdn.jsdelivr.net/npm/unknown@1.0.0/+esm").unwrap();
        let response = loader.load(&specifier, None, false, RequestedModuleType::None);
        // Should be a synchronous error in strict mode
        assert!(matches!(response, ModuleLoadResponse::Sync(Err(_))));
    }
}
