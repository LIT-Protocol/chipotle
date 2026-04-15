use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use base64::Engine;
use deno_core::error::ModuleLoaderError;
use deno_core::{
    ModuleLoadResponse, ModuleLoader, ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType,
    RequestedModuleType, ResolutionKind,
};
use deno_error::JsErrorBox;
use futures::FutureExt;
use sha2::{Digest, Sha384};
use tracing::{debug, error, info, instrument, warn};

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

/// Truncate a string for log output without panicking on UTF-8 boundaries.
fn truncate_for_log(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last char boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Allowed CDN URL prefix for module imports (origin only, used for URL construction).
const ALLOWED_CDN_PREFIX: &str = "https://cdn.jsdelivr.net/";

/// Allowed CDN URL prefix restricted to the npm backend.
/// Only npm packages are permitted; other jsDelivr backends (/gh/, /wp/, etc.)
/// serve mutable content and are rejected.
const ALLOWED_NPM_PREFIX: &str = "https://cdn.jsdelivr.net/npm/";

/// Maximum response body size (10 MB).
const MAX_MODULE_SIZE_BYTES: usize = 10 * 1024 * 1024;

/// Maximum total cached bytes before evicting oldest entries (100 MB).
const MAX_CACHE_BYTES: usize = 100 * 1024 * 1024;

/// HTTP request timeout.
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum number of distinct modules that can be loaded per action execution.
/// Prevents DoS via dependency graphs with thousands of tiny files.
const MAX_MODULE_COUNT: usize = 100;

/// Thread-safe cache for fetched and integrity-verified module sources.
pub type ModuleCache = Arc<RwLock<HashMap<String, Vec<u8>>>>;

// Re-export from ext crate for convenience.
pub use lit_actions_ext::bindings::{LoadedModuleInfo, LoadedModules};

pub struct CdnModuleLoader {
    /// Maps CDN URLs to their expected base64-encoded SHA-384 hashes.
    integrity: Arc<RwLock<HashMap<String, String>>>,
    /// If true (production), unknown modules require TOFU double-fetch verification.
    /// If false (test/dev), unknown modules are accepted after a single fetch.
    strict: bool,
    /// Reusable HTTP client with timeouts and redirect policy.
    /// Shared across all loader instances to enable connection pooling.
    client: Arc<reqwest::Client>,
    /// Cache of fetched module sources, shared across loader instances.
    /// Bounded by MAX_CACHE_BYTES; oldest entries evicted when full.
    cache: ModuleCache,
    /// Path to integrity.lock file on disk. When set, enables trust-on-first-use:
    /// new modules are double-fetched, verified, and pinned to the lockfile.
    lockfile_path: Option<PathBuf>,
    /// Per-execution tracker of all loaded modules and their hashes.
    loaded_modules: LoadedModules,
}

impl CdnModuleLoader {
    /// Create a new `CdnModuleLoader`.
    ///
    /// - `integrity`: parsed integrity manifest mapping URL → base64-encoded SHA-384 hash
    /// - `strict`: if true, unknown modules require TOFU double-fetch verification;
    ///   if false, unknown modules are accepted after a single fetch
    pub fn new(integrity: Arc<RwLock<HashMap<String, String>>>, strict: bool) -> Self {
        Self::with_options(
            integrity,
            strict,
            Arc::new(RwLock::new(HashMap::new())),
            None,
            None,
            LoadedModules::default(),
        )
    }

    /// Build the default HTTP client with security-hardened settings.
    pub fn build_http_client() -> Arc<reqwest::Client> {
        Arc::new(
            reqwest::Client::builder()
                .timeout(FETCH_TIMEOUT)
                .connect_timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("failed to build HTTP client"),
        )
    }

    /// Create a new `CdnModuleLoader` with a shared cache, optional lockfile path,
    /// optional shared HTTP client, and per-execution module tracker.
    pub fn with_options(
        integrity: Arc<RwLock<HashMap<String, String>>>,
        strict: bool,
        cache: ModuleCache,
        lockfile_path: Option<PathBuf>,
        client: Option<Arc<reqwest::Client>>,
        loaded_modules: LoadedModules,
    ) -> Self {
        Self {
            integrity,
            strict,
            client: client.unwrap_or_else(Self::build_http_client),
            cache,
            lockfile_path,
            loaded_modules,
        }
    }

    /// Returns the per-execution loaded modules tracker, for sharing with OpState.
    pub fn loaded_modules(&self) -> LoadedModules {
        self.loaded_modules.clone()
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

    /// Check whether a URL is from the allowed CDN npm backend.
    /// Only `https://cdn.jsdelivr.net/npm/` URLs are accepted; other jsDelivr
    /// backends (`/gh/`, `/wp/`, etc.) serve mutable content and are rejected.
    fn is_allowed_cdn(url: &str) -> bool {
        url.starts_with(ALLOWED_NPM_PREFIX)
    }

    /// Parse an npm package specifier into a full jsDelivr URL.
    ///
    /// Accepts these formats:
    /// - `package@version` → `https://cdn.jsdelivr.net/npm/package@version/+esm`
    /// - `package@version/+esm` → `https://cdn.jsdelivr.net/npm/package@version/+esm`
    /// - `package@version/file` → `https://cdn.jsdelivr.net/npm/package@version/file`
    /// - `@scope/package@version` → `https://cdn.jsdelivr.net/npm/@scope/package@version/+esm`
    /// - `@scope/package@version/file` → `https://cdn.jsdelivr.net/npm/@scope/package@version/file`
    ///
    /// When no file path is specified after the version, `/+esm` is automatically
    /// appended to request the ESM entry point from jsDelivr.
    ///
    /// An optional `#sha384-<hash>` fragment is preserved on the output URL for
    /// inline integrity verification.
    ///
    /// Returns None if the specifier doesn't match the expected format.
    fn parse_npm_specifier(specifier: &str) -> Option<String> {
        // Split off the optional #hash fragment before parsing
        let (spec, fragment) = match specifier.split_once('#') {
            Some((s, f)) => (s, Some(f)),
            None => (specifier, None),
        };

        // Must contain @ for version pinning (but not just a leading @ for scoped packages)
        let version_at = if let Some(rest) = spec.strip_prefix('@') {
            // Scoped package: @scope/pkg@version — find the second @
            rest.find('@').map(|i| i + 1)
        } else {
            spec.find('@')
        };

        let version_at = version_at?;

        let after_at = &spec[version_at + 1..];
        if after_at.is_empty() {
            return None;
        }

        // Version must start with a digit
        if !after_at.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }

        // Auto-append /+esm when no file path is specified after the version.
        // This ensures jsDelivr serves the ESM entry point by default.
        let has_path = after_at.contains('/');
        let mut url = if has_path {
            format!("{ALLOWED_CDN_PREFIX}npm/{spec}")
        } else {
            format!("{ALLOWED_CDN_PREFIX}npm/{spec}/+esm")
        };
        if let Some(frag) = fragment {
            url.push('#');
            url.push_str(frag);
        }
        Some(url)
    }

    /// Fetch a URL with streaming body read, enforcing size limits during download.
    /// Returns the response bytes and optionally the SRI hash from CDN headers.
    #[instrument(skip_all, err)]
    async fn fetch_with_size_limit(
        client: &reqwest::Client,
        url: &str,
        label: &str,
    ) -> Result<(Vec<u8>, Option<String>), ModuleLoaderError> {
        let response = client.get(url).send().await.map_err(|e| {
            error!(module_url = %url, fetch = label, error = %e, "CDN module fetch failed");
            JsErrorBox::generic(format!("{label} fetch failed for {url}: {e}"))
        })?;

        let status = response.status();
        if status.is_redirection() {
            error!(module_url = %url, fetch = label, http_status = %status, "CDN module fetch rejected: redirect");
            return Err(JsErrorBox::generic(format!(
                "Module {url} returned a redirect (HTTP {status}). \
                 Redirects are not allowed for CDN module imports."
            ))
            .into());
        }

        if !status.is_success() {
            error!(module_url = %url, fetch = label, http_status = %status, "CDN module fetch failed: non-success status");
            return Err(JsErrorBox::generic(format!(
                "{label} fetch failed for {url}: HTTP {status}"
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
            debug!(module_url = %url, fetch = label, cdn_sri = %format!("sha384-{sri}"), "CDN provided SRI hash");
        }

        // Pre-check content-length header
        if let Some(len) = response.content_length()
            && len as usize > MAX_MODULE_SIZE_BYTES
        {
            error!(module_url = %url, fetch = label, content_length = len, max_bytes = MAX_MODULE_SIZE_BYTES, "CDN module rejected: exceeds size limit");
            return Err(JsErrorBox::generic(format!(
                "Module {url} exceeds maximum size ({len} bytes > {MAX_MODULE_SIZE_BYTES} bytes)"
            ))
            .into());
        }

        // Stream body with hard size cap to prevent OOM even if content-length is absent/wrong
        let mut bytes = Vec::new();
        let mut response = response;
        while let Some(chunk) = response.chunk().await.map_err(|e| {
            error!(module_url = %url, fetch = label, error = %e, "CDN module fetch: failed to read chunk");
            JsErrorBox::generic(format!("Failed to read response body for {url}: {e}"))
        })? {
            if bytes.len() + chunk.len() > MAX_MODULE_SIZE_BYTES {
                error!(module_url = %url, fetch = label, body_size = bytes.len() + chunk.len(), max_bytes = MAX_MODULE_SIZE_BYTES, "CDN module rejected: body exceeds size limit during streaming");
                return Err(JsErrorBox::generic(format!(
                    "Module {url} exceeds maximum size (> {MAX_MODULE_SIZE_BYTES} bytes)"
                ))
                .into());
            }
            bytes.extend_from_slice(&chunk);
        }

        info!(module_url = %url, fetch = label, size_bytes = bytes.len(), "CDN module fetch: download complete");
        Ok((bytes, cdn_sri_hash))
    }
}

impl ModuleLoader for CdnModuleLoader {
    #[instrument(skip_all, err)]
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, ModuleLoaderError> {
        // Resolve relative imports against the referrer when the referrer is a jsDelivr npm URL.
        // ESM modules on jsDelivr can have relative imports between files in the same package.
        if (specifier.starts_with("./") || specifier.starts_with("../"))
            && Self::is_allowed_cdn(referrer)
        {
            let base = ModuleSpecifier::parse(referrer).map_err(|e| {
                JsErrorBox::generic(format!("Invalid referrer URL: {referrer}: {e}"))
            })?;
            let resolved = base.join(specifier).map_err(|e| {
                JsErrorBox::generic(format!(
                    "Failed to resolve relative import \"{specifier}\" against {referrer}: {e}"
                ))
            })?;
            // Verify the resolved URL stays within the /npm/ backend.
            // Relative traversal (../../) could escape to /gh/ or other backends.
            if Self::is_allowed_cdn(resolved.as_str()) {
                info!(
                    specifier,
                    referrer,
                    resolved_url = %resolved,
                    "CDN module resolve: relative import resolved against jsDelivr referrer"
                );
                return Ok(resolved);
            }
            warn!(
                specifier,
                referrer,
                resolved_url = %resolved,
                "CDN module resolve rejected: relative import resolved outside /npm/ boundary"
            );
            return Err(JsErrorBox::generic(format!(
                "Relative import \"{specifier}\" resolved to {resolved}, which is outside the allowed /npm/ CDN path"
            ))
            .into());
        }

        // If it's already a full jsDelivr URL, pass through
        if Self::is_allowed_cdn(specifier) {
            info!(specifier, "CDN module resolve: full URL accepted");
            return ModuleSpecifier::parse(specifier).map_err(|e| {
                JsErrorBox::generic(format!("Invalid module URL: {specifier}: {e}")).into()
            });
        }

        // Allow data: URIs with JavaScript MIME type when the referrer is a CDN module.
        // jsDelivr ESM bundles inline small dependencies as data:text/javascript;base64,... imports.
        if (specifier.starts_with("data:text/javascript;") || specifier.starts_with("data:text/javascript,"))
            && Self::is_allowed_cdn(referrer)
        {
            info!(
                specifier = %truncate_for_log(specifier, 80),
                referrer,
                "CDN module resolve: data: URI accepted (inlined dependency from CDN module)"
            );
            return ModuleSpecifier::parse(specifier).map_err(|e| {
                JsErrorBox::generic(format!("Invalid data: URI: {e}")).into()
            });
        }

        // Try parsing as an npm specifier (e.g. "zod@3.22.4/+esm")
        if let Some(cdn_url) = Self::parse_npm_specifier(specifier) {
            info!(
                specifier,
                resolved_url = %cdn_url,
                "CDN module resolve: npm specifier resolved to jsDelivr URL"
            );
            return ModuleSpecifier::parse(&cdn_url).map_err(|e| {
                JsErrorBox::generic(format!("Invalid resolved URL: {cdn_url}: {e}")).into()
            });
        }

        // Reject everything else
        warn!(
            specifier,
            referrer, "CDN module resolve rejected: not a valid npm specifier or jsDelivr URL"
        );
        Err(JsErrorBox::generic(format!(
            "Invalid import specifier: \"{specifier}\". \
             Use an npm specifier with a pinned version (e.g. zod@3.22.4) \
             or a full jsDelivr URL (e.g. https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm)"
        ))
        .into())
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        _maybe_referrer: Option<&ModuleSpecifier>,
        _is_dyn_import: bool,
        _requested_module_type: RequestedModuleType,
    ) -> ModuleLoadResponse {
        // Enforce per-execution module count limit to prevent DoS via
        // dependency graphs with thousands of tiny files.
        if let Ok(modules) = self.loaded_modules.0.read()
            && modules.len() >= MAX_MODULE_COUNT
        {
            error!(
                module_count = modules.len(),
                max = MAX_MODULE_COUNT,
                "CDN module load rejected: maximum module count exceeded"
            );
            return ModuleLoadResponse::Sync(Err(JsErrorBox::generic(format!(
                "Maximum module count ({MAX_MODULE_COUNT}) exceeded. \
                     Reduce the number of imported modules."
            ))
            .into()));
        }

        // Handle data: URIs inline — these are inlined dependencies from CDN ESM bundles.
        // Decode the base64 content directly instead of attempting an HTTP fetch.
        let url_str = module_specifier.as_str();
        if url_str.starts_with("data:text/javascript;") || url_str.starts_with("data:text/javascript,") {
            // Pre-check raw URI body length to avoid large transient allocations during decode.
            // Base64 expands ~33%, so the raw body can be up to 4/3 of the decoded size limit.
            let max_raw_len = MAX_MODULE_SIZE_BYTES * 4 / 3 + 4;
            let body_start = url_str.find(',').map(|i| i + 1).unwrap_or(url_str.len());
            if url_str.len() - body_start > max_raw_len {
                return ModuleLoadResponse::Sync(Err(JsErrorBox::generic(format!(
                    "data: URI body exceeds maximum size ({} bytes > {max_raw_len} bytes)",
                    url_str.len() - body_start
                ))
                .into()));
            }

            let data_body = if let Some(rest) = url_str.strip_prefix("data:text/javascript;base64,")
            {
                base64::engine::general_purpose::STANDARD
                    .decode(rest)
                    .map_err(|e| {
                        JsErrorBox::generic(format!("Invalid base64 in data: URI: {e}"))
                    })
            } else if let Some(rest) = url_str.strip_prefix("data:text/javascript,") {
                // Plain (non-base64) data URI — percent-decode the body
                Ok(percent_encoding::percent_decode_str(rest).collect::<Vec<u8>>())
            } else {
                Err(JsErrorBox::generic(format!(
                    "Unsupported data: URI encoding: {}",
                    truncate_for_log(url_str, 80)
                )))
            };

            return match data_body {
                Ok(bytes) => {
                    if bytes.len() > MAX_MODULE_SIZE_BYTES {
                        ModuleLoadResponse::Sync(Err(JsErrorBox::generic(format!(
                            "data: URI exceeds maximum module size ({} bytes > {MAX_MODULE_SIZE_BYTES} bytes)",
                            bytes.len()
                        )).into()))
                    } else {
                        // Track data URI in loaded_modules so it counts toward MAX_MODULE_COUNT
                        // and appears in showImportDetails(). Use a hash of the content as the
                        // dedup key to prevent same-length data URIs from collapsing into one entry.
                        if let Ok(mut modules) = self.loaded_modules.0.write() {
                            let mut hasher = Sha384::new();
                            hasher.update(&bytes);
                            let hash = base64::engine::general_purpose::STANDARD.encode(hasher.finalize());
                            let data_key = format!("data:text/javascript;sha384-{}", &hash[..16]);
                            if !modules.iter().any(|m| m.url == data_key) {
                                modules.push(LoadedModuleInfo {
                                    url: data_key,
                                    hash,
                                });
                            }
                        }
                        info!(
                            module_url = %truncate_for_log(url_str, 80),
                            size_bytes = bytes.len(),
                            "CDN module loaded from data: URI"
                        );
                        ModuleLoadResponse::Sync(Ok(ModuleSource::new(
                            ModuleType::JavaScript,
                            ModuleSourceCode::Bytes(bytes.into_boxed_slice().into()),
                            module_specifier,
                            None,
                        )))
                    }
                }
                Err(e) => ModuleLoadResponse::Sync(Err(e.into())),
            };
        }

        // Extract inline hash from URL fragment (e.g. #sha384-abc123...)
        let inline_hash = module_specifier
            .fragment()
            .and_then(|f| f.strip_prefix("sha384-"))
            .map(|h| h.to_string());

        // Build the fetch URL without the fragment
        let mut fetch_url = module_specifier.clone();
        fetch_url.set_fragment(None);
        let url = fetch_url.to_string();

        if let Some(ref h) = inline_hash {
            info!(
                module_url = %url,
                inline_hash = %format!("sha384-{h}"),
                "CDN module load: inline integrity hash provided in import specifier"
            );
        }

        // Inline hash takes priority, then lockfile manifest
        let expected_hash = inline_hash.clone().or_else(|| {
            self.integrity
                .read()
                .expect("integrity lock poisoned")
                .get(&url)
                .cloned()
        });

        // In strict mode, unknown modules (not in manifest and no inline hash)
        // are verified via TOFU (trust-on-first-use): double-fetch + CDN SRI
        // header check. If a lockfile path is configured, the verified hash is
        // also persisted to disk; otherwise the pin lives only in memory for
        // the lifetime of this process.

        // Check cache first
        if let Ok(cache) = self.cache.read()
            && let Some(cached_bytes) = cache.get(&url)
        {
            // If an expected hash exists (inline or manifest), verify it against
            // the actual cached bytes. This catches both stale manifest entries and
            // inline hash mismatches, even when the URL has no manifest entry.
            if let Some(ref expected_b64) = expected_hash {
                let mut hasher = Sha384::new();
                hasher.update(cached_bytes);
                let cached_digest = hasher.finalize();
                let cached_b64 = base64::engine::general_purpose::STANDARD.encode(cached_digest);
                if !constant_time_eq(cached_b64.as_bytes(), expected_b64.as_bytes()) {
                    error!(
                        module_url = %url,
                        expected_hash = %format!("sha384-{expected_b64}"),
                        cached_hash = %format!("sha384-{cached_b64}"),
                        "CDN module cache: integrity check failed for cached bytes"
                    );
                    return ModuleLoadResponse::Sync(Err(JsErrorBox::generic(format!(
                        "Integrity check failed for cached {url}: \
                             expected sha384-{expected_b64}, got sha384-{cached_b64}"
                    ))
                    .into()));
                }
            }

            // Record the loaded module for showImportDetails().
            // Use expected_hash (inline or manifest) when available, so inline-hash-only
            // imports don't lose the declared hash.
            let hash = expected_hash
                .clone()
                .or_else(|| {
                    self.integrity
                        .read()
                        .expect("integrity lock poisoned")
                        .get(&url)
                        .cloned()
                })
                .unwrap_or_default();
            if let Ok(mut modules) = self.loaded_modules.0.write()
                && !modules.iter().any(|m| m.url == url)
            {
                modules.push(LoadedModuleInfo {
                    url: url.clone(),
                    hash,
                });
            }
            debug!(module_url = %url, size_bytes = cached_bytes.len(), "CDN module loaded from cache");
            return ModuleLoadResponse::Sync(Ok(ModuleSource::new(
                ModuleType::JavaScript,
                ModuleSourceCode::Bytes(cached_bytes.clone().into_boxed_slice().into()),
                module_specifier,
                None,
            )));
        }

        let client = self.client.clone();
        let cache = self.cache.clone();
        let integrity = self.integrity.clone();
        let lockfile_path = self.lockfile_path.clone();
        let strict = self.strict;
        let loaded_modules = self.loaded_modules.clone();

        let fut = async move {
            info!(module_url = %url, "CDN module fetch: downloading");

            let (bytes, cdn_sri_hash) =
                Self::fetch_with_size_limit(&client, &url, "Primary").await?;

            let mut hasher = Sha384::new();
            hasher.update(&bytes);
            let actual_digest = hasher.finalize();

            let actual_b64 = base64::engine::general_purpose::STANDARD.encode(actual_digest);

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
            } else if strict {
                // Unknown module in strict mode: trust-on-first-use (TOFU)
                // Double-fetch + CDN SRI header verification.
                info!(
                    module_url = %url,
                    first_hash = %format!("sha384-{actual_b64}"),
                    "TOFU: new module detected, starting verification fetch"
                );

                // Second fetch with identical protections (redirect/status/size checks)
                let (bytes2, _) =
                    Self::fetch_with_size_limit(&client, &url, "TOFU verification").await?;

                let mut hasher2 = Sha384::new();
                hasher2.update(&bytes2);
                let verify_digest = hasher2.finalize();
                let verify_b64 =
                    base64::engine::general_purpose::STANDARD.encode(verify_digest);

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

                // Verify against CDN's SRI hash header if available
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

                // Pin to lockfile on disk.
                // Note: each JS execution runs on its own dedicated thread via
                // create_and_run_current_thread, so blocking I/O here does not starve
                // other async work. Concurrent appends are safe because we re-check
                // the in-memory map under write lock before writing.
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
            } else {
                // Non-strict mode: pin hash from single fetch without TOFU verification.
                // Used in test/development environments for faster iteration.
                info!(
                    module_url = %url,
                    hash = %format!("sha384-{actual_b64}"),
                    size_bytes = bytes.len(),
                    "Non-strict mode: accepting module without TOFU verification"
                );

                if let Ok(mut map) = integrity.write() {
                    map.entry(url.clone()).or_insert(actual_b64.clone());
                }
            }

            // Record the loaded module for showImportDetails() (dedup by URL)
            if let Ok(mut modules) = loaded_modules.0.write()
                && !modules.iter().any(|m| m.url == url)
            {
                modules.push(LoadedModuleInfo {
                    url: url.clone(),
                    hash: actual_b64.clone(),
                });
            }

            // Cache the verified module source (bounded by MAX_CACHE_BYTES)
            if let Ok(mut cache_w) = cache.write() {
                let total: usize = cache_w.values().map(|v| v.len()).sum();
                if total + bytes.len() <= MAX_CACHE_BYTES {
                    cache_w.insert(url.clone(), bytes.clone());
                    debug!(module_url = %url, "CDN module cached for future requests");
                } else {
                    warn!(module_url = %url, cache_bytes = total, module_bytes = bytes.len(), max_cache = MAX_CACHE_BYTES, "CDN module cache full, skipping cache insertion");
                }
            }

            info!(module_url = %url, size_bytes = bytes.len(), "CDN module loaded successfully");

            let module_specifier = ModuleSpecifier::parse(&url).map_err(|e| {
                JsErrorBox::generic(format!("Invalid module URL during load: {url}: {e}"))
            })?;

            Ok(ModuleSource::new(
                ModuleType::JavaScript,
                ModuleSourceCode::Bytes(bytes.into_boxed_slice().into()),
                &module_specifier,
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
    fn test_parse_npm_specifier() {
        // Bare specifiers without a path get /+esm auto-appended
        assert_eq!(
            CdnModuleLoader::parse_npm_specifier("zod@3.22.4"),
            Some("https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm".to_string())
        );
        // Explicit /+esm is preserved as-is
        assert_eq!(
            CdnModuleLoader::parse_npm_specifier("zod@3.22.4/+esm"),
            Some("https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm".to_string())
        );
        assert_eq!(
            CdnModuleLoader::parse_npm_specifier("@scope/pkg@1.0.0/+esm"),
            Some("https://cdn.jsdelivr.net/npm/@scope/pkg@1.0.0/+esm".to_string())
        );
        // Scoped package without path gets /+esm auto-appended
        assert_eq!(
            CdnModuleLoader::parse_npm_specifier("@scope/pkg@2.0.0"),
            Some("https://cdn.jsdelivr.net/npm/@scope/pkg@2.0.0/+esm".to_string())
        );
        // Explicit file path is preserved (no /+esm appended)
        assert_eq!(
            CdnModuleLoader::parse_npm_specifier("date-fns@3.6.0/esm/index.js"),
            Some("https://cdn.jsdelivr.net/npm/date-fns@3.6.0/esm/index.js".to_string())
        );
        assert_eq!(
            CdnModuleLoader::parse_npm_specifier("zod@3.22.4/+esm#sha384-abc123"),
            Some("https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm#sha384-abc123".to_string())
        );
        assert_eq!(
            CdnModuleLoader::parse_npm_specifier("@scope/pkg@1.0.0/+esm#sha384-xyz789"),
            Some("https://cdn.jsdelivr.net/npm/@scope/pkg@1.0.0/+esm#sha384-xyz789".to_string())
        );
        // Bare specifier with hash fragment gets /+esm auto-appended
        assert_eq!(
            CdnModuleLoader::parse_npm_specifier("zod@3.22.4#sha384-abc123"),
            Some("https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm#sha384-abc123".to_string())
        );
        assert_eq!(CdnModuleLoader::parse_npm_specifier("lodash-es"), None);
        assert_eq!(CdnModuleLoader::parse_npm_specifier("./local.js"), None);
        assert_eq!(CdnModuleLoader::parse_npm_specifier("@scope/pkg"), None);
    }

    #[test]
    fn test_resolve_npm_specifiers() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        // Explicit /+esm
        let result = loader
            .resolve("zod@3.22.4/+esm", "file:///main.js", ResolutionKind::Import)
            .unwrap();
        assert_eq!(
            result.as_str(),
            "https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm"
        );
        // Bare specifier — /+esm auto-appended
        let result = loader
            .resolve("zod@3.22.4", "file:///main.js", ResolutionKind::Import)
            .unwrap();
        assert_eq!(
            result.as_str(),
            "https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm"
        );
    }

    #[test]
    fn test_resolve_full_cdn_urls() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        let result = loader
            .resolve(
                "https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm",
                "file:///main.js",
                ResolutionKind::Import,
            )
            .unwrap();
        assert_eq!(
            result.as_str(),
            "https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm"
        );
    }

    #[test]
    fn test_resolve_relative_imports() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        let result = loader
            .resolve(
                "./utils.js",
                "https://cdn.jsdelivr.net/npm/zod@3.22.4/lib/index.js",
                ResolutionKind::Import,
            )
            .unwrap();
        assert_eq!(
            result.as_str(),
            "https://cdn.jsdelivr.net/npm/zod@3.22.4/lib/utils.js"
        );
    }

    #[test]
    fn test_resolve_relative_rejects_non_cdn_referrer() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        assert!(
            loader
                .resolve("./utils.js", "file:///main.js", ResolutionKind::Import)
                .is_err()
        );
    }

    #[test]
    fn test_resolve_with_inline_hash() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        let result = loader
            .resolve(
                "zod@3.22.4/+esm#sha384-abc123def",
                "file:///main.js",
                ResolutionKind::Import,
            )
            .unwrap();
        assert_eq!(
            result.as_str(),
            "https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm#sha384-abc123def"
        );
        assert_eq!(result.fragment(), Some("sha384-abc123def"));
    }

    #[test]
    fn test_resolve_rejects_bare_specifiers() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        assert!(
            loader
                .resolve("lodash-es", "file:///main.js", ResolutionKind::Import)
                .is_err()
        );
    }

    #[test]
    fn test_resolve_rejects_other_urls() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        assert!(
            loader
                .resolve(
                    "https://evil.com/malware.js",
                    "file:///main.js",
                    ResolutionKind::Import
                )
                .is_err()
        );
    }

    #[test]
    fn test_resolve_rejects_jsdelivr_gh_backend() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        // Direct /gh/ URL should be rejected even though it's on jsdelivr
        assert!(
            loader
                .resolve(
                    "https://cdn.jsdelivr.net/gh/user/repo@main/file.js",
                    "file:///main.js",
                    ResolutionKind::Import
                )
                .is_err()
        );
    }

    #[test]
    fn test_resolve_relative_rejects_npm_escape_to_gh() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        // Relative traversal that escapes from /npm/ into /gh/ should be rejected
        assert!(
            loader
                .resolve(
                    "../../../gh/user/repo@main/file.js",
                    "https://cdn.jsdelivr.net/npm/pkg@1.0.0/lib/index.js",
                    ResolutionKind::Import
                )
                .is_err()
        );
    }

    #[test]
    fn test_resolve_data_uri_from_cdn_referrer() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        let data_uri = "data:text/javascript;base64,ZXhwb3J0IGRlZmF1bHQgNDI7";
        let result = loader
            .resolve(
                data_uri,
                "https://cdn.jsdelivr.net/npm/micro-eth-signer@0.18.1/+esm",
                ResolutionKind::Import,
            )
            .unwrap();
        assert_eq!(result.as_str(), data_uri);
    }

    #[test]
    fn test_resolve_data_uri_rejected_from_non_cdn_referrer() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        assert!(loader
            .resolve(
                "data:text/javascript;base64,ZXhwb3J0IGRlZmF1bHQgNDI7",
                "file:///main.js",
                ResolutionKind::Import,
            )
            .is_err());
    }

    #[test]
    fn test_resolve_data_uri_rejected_non_javascript() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        // data:text/html should be rejected even with CDN referrer
        assert!(loader
            .resolve(
                "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
                "https://cdn.jsdelivr.net/npm/pkg@1.0.0/+esm",
                ResolutionKind::Import,
            )
            .is_err());
    }

    #[test]
    fn test_load_data_uri_base64() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        // "export default 42;" base64-encoded
        let specifier = ModuleSpecifier::parse(
            "data:text/javascript;base64,ZXhwb3J0IGRlZmF1bHQgNDI7",
        )
        .unwrap();
        let response = loader.load(&specifier, None, false, RequestedModuleType::None);
        match response {
            ModuleLoadResponse::Sync(Ok(source)) => {
                let code = match &source.code {
                    ModuleSourceCode::Bytes(b) => String::from_utf8_lossy(b.as_bytes()).to_string(),
                    _ => panic!("expected Bytes"),
                };
                assert_eq!(code, "export default 42;");
            }
            ModuleLoadResponse::Sync(Err(e)) => panic!("expected Ok, got Err: {e:?}"),
            ModuleLoadResponse::Async(_) => panic!("expected Sync response for data: URI"),
        }
    }

    #[test]
    fn test_load_data_uri_plain() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        let specifier =
            ModuleSpecifier::parse("data:text/javascript,export%20default%2042%3B").unwrap();
        let response = loader.load(&specifier, None, false, RequestedModuleType::None);
        match response {
            ModuleLoadResponse::Sync(Ok(source)) => {
                let code = match &source.code {
                    ModuleSourceCode::Bytes(b) => String::from_utf8_lossy(b.as_bytes()).to_string(),
                    _ => panic!("expected Bytes"),
                };
                assert_eq!(code, "export default 42;");
            }
            ModuleLoadResponse::Sync(Err(e)) => panic!("expected Ok, got Err: {e:?}"),
            ModuleLoadResponse::Async(_) => panic!("expected Sync response for data: URI"),
        }
    }

    #[test]
    fn test_load_data_uri_invalid_base64() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        let specifier =
            ModuleSpecifier::parse("data:text/javascript;base64,NOT_VALID!!!").unwrap();
        let response = loader.load(&specifier, None, false, RequestedModuleType::None);
        assert!(
            matches!(response, ModuleLoadResponse::Sync(Err(_))),
            "invalid base64 should return an error"
        );
    }

    #[test]
    fn test_load_data_uri_unsupported_encoding() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        // charset parameter before base64 is a valid data URI format but unsupported
        let specifier =
            ModuleSpecifier::parse("data:text/javascript;charset=utf-8;base64,ZXhwb3J0IGRlZmF1bHQgNDI7").unwrap();
        let response = loader.load(&specifier, None, false, RequestedModuleType::None);
        assert!(
            matches!(response, ModuleLoadResponse::Sync(Err(_))),
            "unsupported data URI encoding should return an error"
        );
    }

    #[test]
    fn test_resolve_data_uri_prefix_boundary() {
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), false);
        // "data:text/javascript-evil" should NOT match — only data:text/javascript; or data:text/javascript,
        assert!(loader
            .resolve(
                "data:text/javascript-evil;base64,ZXhwb3J0IGRlZmF1bHQgNDI7",
                "https://cdn.jsdelivr.net/npm/pkg@1.0.0/+esm",
                ResolutionKind::Import,
            )
            .is_err());
    }

    #[test]
    fn test_strict_mode_allows_tofu_for_unknown_modules() {
        // Even without a lockfile, strict mode should fall through to async
        // TOFU verification instead of rejecting synchronously.
        let loader = CdnModuleLoader::new(Arc::new(RwLock::new(HashMap::new())), true);
        let specifier =
            ModuleSpecifier::parse("https://cdn.jsdelivr.net/npm/unknown@1.0.0/+esm").unwrap();
        let response = loader.load(&specifier, None, false, RequestedModuleType::None);
        assert!(
            matches!(response, ModuleLoadResponse::Async(_)),
            "strict mode without lockfile should use async TOFU, not reject synchronously"
        );
    }

    #[test]
    fn test_strict_mode_serves_known_module_from_cache() {
        // A module already in the manifest + cache should still load synchronously.
        let mut manifest = HashMap::new();
        manifest.insert(
            "https://cdn.jsdelivr.net/npm/known@1.0.0/+esm".to_string(),
            // any base64 value — the cache path doesn't re-verify
            "dGVzdA==".to_string(),
        );
        let cache: ModuleCache = Arc::new(RwLock::new(HashMap::new()));
        cache.write().unwrap().insert(
            "https://cdn.jsdelivr.net/npm/known@1.0.0/+esm".to_string(),
            b"export default 42;\n".to_vec(),
        );
        let loader = CdnModuleLoader::with_options(
            Arc::new(RwLock::new(manifest)),
            true,
            cache,
            None,
            None,
            LoadedModules::default(),
        );
        let specifier =
            ModuleSpecifier::parse("https://cdn.jsdelivr.net/npm/known@1.0.0/+esm").unwrap();
        let response = loader.load(&specifier, None, false, RequestedModuleType::None);
        assert!(
            matches!(response, ModuleLoadResponse::Sync(Ok(_))),
            "known cached module should load synchronously"
        );
    }
}
