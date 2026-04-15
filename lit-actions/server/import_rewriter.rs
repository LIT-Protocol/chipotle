//! Rewrites static ES module `import` statements into dynamic `import()` calls.
//!
//! The Deno runtime executes user code in **script mode** via `execute_script()`,
//! which does not support static `import` declarations (an ES module feature).
//! This module scans user code for static imports that appear before
//! `async function main`, strips them from the source, and returns structured
//! data that the runtime uses to generate equivalent dynamic `import()` calls
//! inside the async wrapper.
//!
//! Additionally, the bundling pipeline (`bundle_imports`) recursively fetches all
//! transitive dependencies from jsDelivr and inlines them as `data:` URLs so that
//! no module loader network I/O is needed at runtime.

/// A single binding from an import statement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ImportBinding {
    /// `import foo from "..."`
    Default(String),
    /// `import { a }` or `import { a as b }`
    Named { imported: String, local: String },
    /// `import * as foo from "..."`
    Namespace(String),
}

/// A parsed ES module import statement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedImport {
    /// The module specifier (the string inside quotes after `from`).
    pub specifier: String,
    /// The import bindings (empty for side-effect-only imports).
    pub bindings: Vec<ImportBinding>,
}

/// Result of rewriting imports in user code.
pub(crate) struct RewriteResult {
    /// The user code with import statements removed.
    pub code: String,
    /// The parsed imports in order of appearance.
    pub imports: Vec<ParsedImport>,
}

/// Scan user code for static `import` statements before `async function main`,
/// remove them, and return the parsed imports.
///
/// Code after `async function main` (and any non-import code before it) is
/// preserved unchanged. The scanner is aware of single-line comments (`//`),
/// block comments (`/* */`), and string/template literals so that import-like
/// text inside those constructs is never mistakenly rewritten.
pub(crate) fn rewrite_imports(code: &str) -> RewriteResult {
    let main_pos = find_main_declaration(code);
    let preamble = &code[..main_pos];
    let bytes = preamble.as_bytes();

    let mut imports = Vec::new();
    let mut ranges_to_remove: Vec<(usize, usize)> = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        let b = bytes[pos];

        // Skip single-line comments
        if b == b'/' && bytes.get(pos + 1) == Some(&b'/') {
            match preamble[pos..].find('\n') {
                Some(nl) => {
                    pos += nl + 1;
                    continue;
                }
                None => break,
            }
        }

        // Skip block comments
        if b == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            match preamble[pos + 2..].find("*/") {
                Some(end) => {
                    pos += end + 4;
                    continue;
                }
                None => break,
            }
        }

        // Skip string literals
        if b == b'"' || b == b'\'' {
            if let Some(len) = skip_string_literal(&preamble[pos..], b) {
                pos += len;
                continue;
            }
            pos += 1;
            continue;
        }

        // Skip template literals
        if b == b'`' {
            if let Some(len) = skip_template_literal(&preamble[pos..]) {
                pos += len;
                continue;
            }
            pos += 1;
            continue;
        }

        // Check for `import` keyword at a statement boundary
        if b == b'i'
            && preamble[pos..].starts_with("import")
            && !is_ident_byte(bytes.get(pos + 6).copied())
            && is_line_start(preamble, pos)
            && let Some((imp, consumed)) = parse_import_statement(&preamble[pos..])
        {
            let start = pos;
            let mut end = pos + consumed;
            // Consume trailing horizontal whitespace + one newline
            end += count_horizontal_ws(&preamble[end..]);
            if bytes.get(end) == Some(&b'\n') {
                end += 1;
            }
            ranges_to_remove.push((start, end));
            imports.push(imp);
            pos = end;
            continue;
        }

        pos += 1;
    }

    // Rebuild code with import statements removed
    let mut result = String::with_capacity(code.len());
    let mut last = 0;
    for &(start, end) in &ranges_to_remove {
        result.push_str(&code[last..start]);
        last = end;
    }
    result.push_str(&code[last..]);

    RewriteResult {
        code: result,
        imports,
    }
}

/// Check if `pos` is at the start of a line (only whitespace between the
/// previous newline and `pos`).
fn is_line_start(s: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let before = &s[..pos];
    for b in before.bytes().rev() {
        match b {
            b'\n' => return true,
            b' ' | b'\t' | b'\r' => continue,
            _ => return false,
        }
    }
    true // beginning of string
}

/// Find the byte offset of the real `async function main` declaration,
/// skipping occurrences inside comments, strings, and template literals.
/// Also requires `main` to be followed by a non-identifier character
/// (rejects `main2`, `mainHelper`, etc.).
fn find_main_declaration(code: &str) -> usize {
    const NEEDLE: &[u8] = b"async function main";
    let bytes = code.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        // Skip single-line comments
        if bytes[pos] == b'/' && bytes.get(pos + 1) == Some(&b'/') {
            match code[pos..].find('\n') {
                Some(nl) => {
                    pos += nl + 1;
                    continue;
                }
                None => return code.len(),
            }
        }

        // Skip block comments
        if bytes[pos] == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            match code[pos + 2..].find("*/") {
                Some(end) => {
                    pos += end + 4;
                    continue;
                }
                None => return code.len(),
            }
        }

        // Skip string literals
        if bytes[pos] == b'"' || bytes[pos] == b'\'' {
            if let Some(len) = skip_string_literal(&code[pos..], bytes[pos]) {
                pos += len;
                continue;
            }
            pos += 1;
            continue;
        }

        // Skip template literals
        if bytes[pos] == b'`' {
            if let Some(len) = skip_template_literal(&code[pos..]) {
                pos += len;
                continue;
            }
            pos += 1;
            continue;
        }

        // Check for the needle at this position
        if bytes[pos..].starts_with(NEEDLE)
            && !is_ident_byte(bytes.get(pos + NEEDLE.len()).copied())
        {
            return pos;
        }

        pos += 1;
    }

    code.len()
}

/// Generate JavaScript code that performs dynamic `import()` calls and
/// destructures the results into local `const` bindings.
///
/// Each import becomes one or two statements:
/// - Side-effect: `await import("specifier");`
/// - Named/default/namespace: `const { a, b: c, default: D } = await import("specifier");`
/// - Namespace: `const Mod = await import("specifier");`
#[cfg(test)]
pub(crate) fn generate_dynamic_imports(imports: &[ParsedImport]) -> String {
    let mut out = String::new();
    for imp in imports {
        let spec = js_escape(&imp.specifier);

        if imp.bindings.is_empty() {
            // Side-effect import
            out.push_str(&format!("await import(\"{spec}\");\n"));
            continue;
        }

        // Check if there's exactly one namespace binding (no destructuring needed)
        if imp.bindings.len() == 1
            && let ImportBinding::Namespace(ref name) = imp.bindings[0]
        {
            out.push_str(&format!("const {name} = await import(\"{spec}\");\n"));
            continue;
        }

        // Destructuring: const { ... } = await import("...");
        // When both a namespace and named/default bindings exist (e.g.
        // `import Default, * as NS from "..."`), import once into a temp
        // variable and assign both from it to avoid a redundant fetch.
        let mut parts = Vec::new();
        let mut has_namespace = false;
        let mut ns_name = String::new();

        for binding in &imp.bindings {
            match binding {
                ImportBinding::Default(name) => {
                    if name == "default" {
                        parts.push("default".to_string());
                    } else {
                        parts.push(format!("default: {name}"));
                    }
                }
                ImportBinding::Named { imported, local } => {
                    if imported == local {
                        parts.push(imported.clone());
                    } else {
                        parts.push(format!("{imported}: {local}"));
                    }
                }
                ImportBinding::Namespace(name) => {
                    has_namespace = true;
                    ns_name = name.clone();
                }
            }
        }

        if has_namespace && !parts.is_empty() {
            // Mixed namespace + named/default: import once, assign both
            out.push_str(&format!("const {ns_name} = await import(\"{spec}\");\n"));
            out.push_str(&format!("const {{ {} }} = {ns_name};\n", parts.join(", "),));
        } else if !parts.is_empty() {
            out.push_str(&format!(
                "const {{ {} }} = await import(\"{spec}\");\n",
                parts.join(", "),
            ));
        } else if has_namespace {
            out.push_str(&format!("const {ns_name} = await import(\"{spec}\");\n"));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// CDN module bundling pipeline
// ---------------------------------------------------------------------------

use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::sync::{Arc, RwLock};

use base64::Engine;
use deno_core::error::ModuleLoaderError;
use deno_error::JsErrorBox;
use tracing::{info, warn};

use std::path::PathBuf;

use crate::cdn_module_loader::{
    ALLOWED_NPM_PREFIX, CdnModuleLoader, MAX_CACHE_BYTES, MAX_MODULE_COUNT, ModuleCache,
    constant_time_eq,
};

/// A resolved module in the dependency graph.
struct ResolvedModule {
    /// Raw source bytes fetched from jsDelivr.
    source: Vec<u8>,
    /// SHA-384 hash (base64) of the original bytes.
    hash: String,
    /// Nested import specifier locations: (byte range of the string content, resolved URL).
    nested_imports: Vec<(Range<usize>, String)>,
}

/// Result of the full bundling pipeline.
pub(crate) struct BundleResult {
    /// JavaScript code containing `const { ... } = await import("data:...");`
    /// statements, fully self-contained with all dependencies inlined.
    pub dynamic_imports: String,
    /// All loaded modules and their hashes, for showImportDetails().
    pub loaded_modules: Vec<(String, String)>,
}

/// Scan module source code (from jsDelivr) for import/export specifiers.
///
/// Finds `from"..."`, `from '...'`, `import"..."` (side-effect), and
/// `export...from"..."` patterns. Returns each specifier's byte range
/// (of the quoted string content) and value.
///
/// Handles minified code (no spaces between `from` and quote) and skips
/// strings, comments, and template literals.
pub(crate) fn scan_cdn_imports(source: &str) -> Vec<(Range<usize>, String)> {
    let bytes = source.as_bytes();
    let mut results = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        let b = bytes[pos];

        // Skip single-line comments
        if b == b'/' && bytes.get(pos + 1) == Some(&b'/') {
            match source[pos..].find('\n') {
                Some(nl) => {
                    pos += nl + 1;
                    continue;
                }
                None => break,
            }
        }

        // Skip block comments
        if b == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            match source[pos + 2..].find("*/") {
                Some(end) => {
                    pos += end + 4;
                    continue;
                }
                None => break,
            }
        }

        // Skip string literals (avoid false matches inside strings)
        if b == b'"' || b == b'\'' {
            if let Some(len) = skip_string_literal(&source[pos..], b) {
                pos += len;
                continue;
            }
            pos += 1;
            continue;
        }

        // Skip template literals
        if b == b'`' {
            if let Some(len) = skip_template_literal(&source[pos..]) {
                pos += len;
                continue;
            }
            pos += 1;
            continue;
        }

        // Look for `from` keyword followed by a quoted string
        if b == b'f'
            && source[pos..].starts_with("from")
            && !is_ident_byte(bytes.get(pos + 4).copied())
        {
            // Check that preceding char is not an identifier char (word boundary)
            if pos > 0 && is_ident_byte(Some(bytes[pos - 1])) {
                pos += 1;
                continue;
            }
            let mut j = pos + 4;
            // skip whitespace between `from` and quote
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                let quote = bytes[j];
                let str_start = j + 1;
                let mut str_end = str_start;
                while str_end < bytes.len() && bytes[str_end] != quote {
                    if bytes[str_end] == b'\\' {
                        str_end += 2;
                        continue;
                    }
                    str_end += 1;
                }
                if str_end < bytes.len() {
                    let spec = source[str_start..str_end].to_string();
                    results.push((str_start..str_end, spec));
                    pos = str_end + 1;
                    continue;
                }
            }
        }

        // Look for side-effect `import` followed directly by a quoted string
        // (e.g., `import"/npm/..."` or `import "/npm/..."`)
        if b == b'i'
            && source[pos..].starts_with("import")
            && !is_ident_byte(bytes.get(pos + 6).copied())
        {
            let mut j = pos + 6;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            // Only match if the next char is a quote (side-effect import),
            // not `{`, `*`, or an identifier (which would be a binding import
            // handled by the `from` branch above)
            if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                let quote = bytes[j];
                let str_start = j + 1;
                let mut str_end = str_start;
                while str_end < bytes.len() && bytes[str_end] != quote {
                    if bytes[str_end] == b'\\' {
                        str_end += 2;
                        continue;
                    }
                    str_end += 1;
                }
                if str_end < bytes.len() {
                    let spec = source[str_start..str_end].to_string();
                    results.push((str_start..str_end, spec));
                    pos = str_end + 1;
                    continue;
                }
            }
        }

        pos += 1;
    }

    results
}

/// Resolve a specifier found in a CDN module to a full jsDelivr URL.
///
/// Handles:
/// - `/npm/...` root-relative paths → prepend CDN origin
/// - `./` and `../` relative paths → join against referrer URL
/// - Full `https://cdn.jsdelivr.net/npm/...` URLs → passthrough
/// - npm specifiers like `pkg@version` → convert to jsDelivr URL
///
/// Returns `None` if the specifier cannot be resolved to a valid jsDelivr npm URL.
fn resolve_cdn_specifier(specifier: &str, referrer_url: &str) -> Option<String> {
    // Root-relative /npm/ paths (jsDelivr's +esm output format)
    if let Some(rest) = specifier.strip_prefix("/npm/") {
        let full = format!("{ALLOWED_NPM_PREFIX}{rest}");
        return Some(full);
    }

    // Relative imports against the referrer
    if (specifier.starts_with("./") || specifier.starts_with("../"))
        && referrer_url.starts_with(ALLOWED_NPM_PREFIX)
    {
        if let Ok(base) = url::Url::parse(referrer_url)
            && let Ok(resolved) = base.join(specifier)
        {
            let s = resolved.to_string();
            if s.starts_with(ALLOWED_NPM_PREFIX) {
                return Some(s);
            }
        }
        return None;
    }

    // Full jsDelivr URL passthrough
    if specifier.starts_with(ALLOWED_NPM_PREFIX) {
        return Some(specifier.to_string());
    }

    // npm specifier (e.g., "zod@3.22.4")
    CdnModuleLoader::parse_npm_specifier(specifier)
}

/// Recursively fetch all transitive dependencies from jsDelivr, then inline
/// them bottom-up as `data:text/javascript;base64,...` URLs.
///
/// Returns a `BundleResult` with self-contained dynamic import statements
/// and metadata for all loaded modules.
pub(crate) async fn bundle_imports(
    imports: &[ParsedImport],
    client: &reqwest::Client,
    integrity: &Arc<RwLock<HashMap<String, String>>>,
    strict: bool,
    lockfile_path: &Option<PathBuf>,
    module_cache: &ModuleCache,
) -> Result<BundleResult, ModuleLoaderError> {
    // Phase 1: Build dependency graph via BFS
    let mut graph: HashMap<String, ResolvedModule> = HashMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut visited: HashSet<String> = HashSet::new();

    // Seed queue with user's top-level import specifiers.
    // Track inline #sha384-... hashes separately for integrity verification.
    let mut root_urls: Vec<String> = Vec::new();
    let mut inline_hashes: HashMap<String, String> = HashMap::new();
    for imp in imports {
        match resolve_cdn_specifier(&imp.specifier, "") {
            Some(url) => {
                let fetch_url = url.split('#').next().unwrap_or(&url).to_string();
                // Preserve inline integrity hash from URL fragment
                if let Some(fragment) = url.split_once('#').map(|(_, f)| f)
                    && let Some(hash) = fragment.strip_prefix("sha384-")
                {
                    inline_hashes.insert(fetch_url.clone(), hash.to_string());
                }
                root_urls.push(fetch_url.clone());
                if !visited.contains(&fetch_url) {
                    queue.push_back(fetch_url.clone());
                    visited.insert(fetch_url);
                }
            }
            None => {
                return Err(JsErrorBox::generic(format!(
                    "Unable to resolve import specifier: \"{}\". \
                     Use an npm specifier with a pinned version (e.g. zod@3.22.4) \
                     or a full jsDelivr URL.",
                    imp.specifier
                ))
                .into());
            }
        }
    }

    while let Some(url) = queue.pop_front() {
        if graph.len() >= MAX_MODULE_COUNT {
            return Err(JsErrorBox::generic(format!(
                "Maximum module count ({MAX_MODULE_COUNT}) exceeded during dependency bundling."
            ))
            .into());
        }

        // Check shared module cache first — avoids re-fetching modules already
        // downloaded by a previous Lit Action execution on this node.
        use sha2::{Digest, Sha384};

        let expected_hash = inline_hashes
            .get(&url)
            .cloned()
            .or_else(|| integrity.read().ok().and_then(|map| map.get(&url).cloned()));

        let cached_bytes = module_cache
            .read()
            .ok()
            .and_then(|c| c.get(&url).cloned());

        let (bytes, from_cache) = if let Some(cached) = cached_bytes {
            // Verify cached bytes against expected hash if one exists
            if let Some(ref expected_b64) = expected_hash {
                let mut hasher = Sha384::new();
                hasher.update(&cached);
                let cached_b64 =
                    base64::engine::general_purpose::STANDARD.encode(hasher.finalize());
                if cached_b64 != *expected_b64 {
                    return Err(JsErrorBox::generic(format!(
                        "Integrity check failed for cached {url}: \
                         expected sha384-{expected_b64}, got sha384-{cached_b64}"
                    ))
                    .into());
                }
            }
            info!(module_url = %url, "Bundler: serving module from cache");
            (cached, true)
        } else {
            info!(module_url = %url, "Bundler: fetching module");
            let (fetched, cdn_sri_hash) =
                CdnModuleLoader::fetch_with_size_limit(client, &url, "Bundler").await?;

            // Compute SHA-384 hash
            let mut hasher = Sha384::new();
            hasher.update(&fetched);
            let actual_digest = hasher.finalize();
            let actual_b64 =
                base64::engine::general_purpose::STANDARD.encode(&actual_digest);

            // Integrity verification — mirrors CdnModuleLoader::load()
            if let Some(ref expected_b64) = expected_hash {
                // Known module: verify against stored hash
                let expected_digest = base64::engine::general_purpose::STANDARD
                    .decode(expected_b64)
                    .map_err(|e| {
                        JsErrorBox::generic(format!(
                            "Invalid base64 in integrity manifest for {url}: {e}"
                        ))
                    })?;
                if actual_digest.len() != expected_digest.len()
                    || !constant_time_eq(&actual_digest, &expected_digest)
                {
                    return Err(JsErrorBox::generic(format!(
                        "Integrity check failed for {url}: \
                         expected sha384-{expected_b64}, got sha384-{actual_b64}"
                    ))
                    .into());
                }
                info!(
                    module_url = %url,
                    hash = %format!("sha384-{actual_b64}"),
                    "Bundler: integrity check passed"
                );
            } else if strict {
                // Unknown module in strict mode: TOFU double-fetch verification
                info!(
                    module_url = %url,
                    first_hash = %format!("sha384-{actual_b64}"),
                    "Bundler TOFU: new module detected, starting verification fetch"
                );

                let (bytes2, _) =
                    CdnModuleLoader::fetch_with_size_limit(client, &url, "Bundler TOFU")
                        .await?;
                let mut hasher2 = Sha384::new();
                hasher2.update(&bytes2);
                let verify_digest = hasher2.finalize();

                if !constant_time_eq(&actual_digest, &verify_digest) {
                    return Err(JsErrorBox::generic(format!(
                        "Bundler TOFU: CDN returned inconsistent content for {url}. \
                         Hash mismatch between first and second fetch. Possible tampering."
                    ))
                    .into());
                }

                // Verify against CDN's SRI hash header if available
                if let Some(ref sri_b64) = cdn_sri_hash {
                    let sri_digest = base64::engine::general_purpose::STANDARD
                        .decode(sri_b64)
                        .map_err(|e| {
                            JsErrorBox::generic(format!(
                                "CDN SRI header for {url} contains invalid base64: {e}"
                            ))
                        })?;
                    if !constant_time_eq(&actual_digest, &sri_digest) {
                        return Err(JsErrorBox::generic(format!(
                            "Bundler TOFU: computed hash does not match CDN SRI header for {url}."
                        ))
                        .into());
                    }
                }

                // Pin to lockfile on disk
                if let Some(path) = lockfile_path {
                    use std::io::Write;
                    let mut file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .map_err(|e| {
                            JsErrorBox::generic(format!(
                                "Failed to open integrity lockfile: {e}"
                            ))
                        })?;
                    writeln!(file, "{url} sha384-{actual_b64}").map_err(|e| {
                        JsErrorBox::generic(format!("Failed to write integrity lockfile: {e}"))
                    })?;
                    file.flush().map_err(|e| {
                        JsErrorBox::generic(format!("Failed to flush integrity lockfile: {e}"))
                    })?;
                    info!(
                        module_url = %url,
                        hash = %format!("sha384-{actual_b64}"),
                        "Bundler TOFU: pinned new module to integrity lockfile"
                    );
                }

                // Update in-memory manifest
                if let Ok(mut map) = integrity.write() {
                    map.entry(url.clone()).or_insert(actual_b64.clone());
                }
            } else {
                // Non-strict mode: accept without TOFU, pin hash in memory
                if let Ok(mut map) = integrity.write() {
                    map.entry(url.clone()).or_insert(actual_b64.clone());
                }
            }

            (fetched, false)
        };

        // Compute hash for the graph entry (needed for loaded_modules tracking)
        let mut hasher = Sha384::new();
        hasher.update(&bytes);
        let hash = base64::engine::general_purpose::STANDARD.encode(hasher.finalize());

        // Store verified module in shared cache (bounded by MAX_CACHE_BYTES)
        if !from_cache {
            if let Ok(mut cache_w) = module_cache.write() {
                let total: usize = cache_w.values().map(|v| v.len()).sum();
                if total + bytes.len() <= MAX_CACHE_BYTES {
                    cache_w.insert(url.clone(), bytes.clone());
                }
            }
        }

        // Scan for nested imports (ES modules must be valid UTF-8)
        let source_str = std::str::from_utf8(&bytes)
            .map_err(|e| JsErrorBox::generic(format!("Module {url} is not valid UTF-8: {e}")))?;
        let found_imports = scan_cdn_imports(source_str);

        let mut nested = Vec::new();
        for (range, spec) in found_imports {
            if let Some(resolved) = resolve_cdn_specifier(&spec, &url) {
                let fetch_resolved = resolved.split('#').next().unwrap_or(&resolved).to_string();
                nested.push((range, fetch_resolved.clone()));
                if !visited.contains(&fetch_resolved) {
                    visited.insert(fetch_resolved.clone());
                    queue.push_back(fetch_resolved);
                }
            } else {
                warn!(
                    module_url = %url,
                    nested_specifier = %spec,
                    "Bundler: could not resolve nested import specifier, skipping"
                );
            }
        }

        graph.insert(
            url,
            ResolvedModule {
                source: bytes,
                hash,
                nested_imports: nested,
            },
        );
    }

    // Phase 2: Topological sort (Kahn's algorithm)
    // Build adjacency: module → set of modules it depends on
    let urls: Vec<String> = graph.keys().cloned().collect();
    let url_to_idx: HashMap<&str, usize> = urls
        .iter()
        .enumerate()
        .map(|(i, u)| (u.as_str(), i))
        .collect();
    let n = urls.len();
    let mut in_degree = vec![0usize; n];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n]; // dep → list of modules that import it

    for (i, url) in urls.iter().enumerate() {
        if let Some(module) = graph.get(url) {
            for (_, dep_url) in &module.nested_imports {
                if let Some(&dep_idx) = url_to_idx.get(dep_url.as_str()) {
                    in_degree[i] += 1;
                    dependents[dep_idx].push(i);
                }
            }
        }
    }

    // Start with leaf modules (no dependencies within the graph)
    let mut topo_queue: VecDeque<usize> = VecDeque::new();
    for (i, &deg) in in_degree.iter().enumerate() {
        if deg == 0 {
            topo_queue.push_back(i);
        }
    }

    let mut topo_order: Vec<usize> = Vec::with_capacity(n);
    while let Some(idx) = topo_queue.pop_front() {
        topo_order.push(idx);
        for &dependent in &dependents[idx] {
            in_degree[dependent] -= 1;
            if in_degree[dependent] == 0 {
                topo_queue.push_back(dependent);
            }
        }
    }

    if topo_order.len() != n {
        return Err(JsErrorBox::generic(
            "Circular dependency detected in jsDelivr modules. Cannot bundle.",
        )
        .into());
    }

    // Phase 3: Bottom-up data URL inlining
    let mut data_urls: HashMap<String, String> = HashMap::new();

    for &idx in &topo_order {
        let url = &urls[idx];
        let module = &graph[url];
        let mut source = module.source.clone();

        // Replace nested import specifiers with data URLs, processing from end to start
        // to preserve byte offsets
        let mut replacements: Vec<(Range<usize>, String)> = module
            .nested_imports
            .iter()
            .filter_map(|(range, dep_url)| {
                data_urls
                    .get(dep_url)
                    .map(|data_url| (range.clone(), data_url.clone()))
            })
            .collect();
        // Sort by range start descending so we replace from end to start
        replacements.sort_by(|a, b| b.0.start.cmp(&a.0.start));

        for (range, data_url) in replacements {
            let before = &source[..range.start];
            let after = &source[range.end..];
            let mut new_source = Vec::with_capacity(before.len() + data_url.len() + after.len());
            new_source.extend_from_slice(before);
            new_source.extend_from_slice(data_url.as_bytes());
            new_source.extend_from_slice(after);
            source = new_source;
        }

        // Create data URL
        let encoded = base64::engine::general_purpose::STANDARD.encode(&source);
        let data_url = format!("data:text/javascript;base64,{encoded}");
        data_urls.insert(url.clone(), data_url);
    }

    // Phase 4: Generate dynamic import statements using data URLs
    let mut dynamic_imports = String::new();
    for (imp, root_url) in imports.iter().zip(root_urls.iter()) {
        let data_url = match data_urls.get(root_url) {
            Some(u) => u,
            None => continue,
        };
        let escaped = js_escape(data_url);

        if imp.bindings.is_empty() {
            dynamic_imports.push_str(&format!("await import(\"{escaped}\");\n"));
            continue;
        }

        if imp.bindings.len() == 1
            && let ImportBinding::Namespace(ref name) = imp.bindings[0]
        {
            dynamic_imports.push_str(&format!("const {name} = await import(\"{escaped}\");\n"));
            continue;
        }

        let mut parts = Vec::new();
        let mut has_namespace = false;
        let mut ns_name = String::new();

        for binding in &imp.bindings {
            match binding {
                ImportBinding::Default(name) => {
                    if name == "default" {
                        parts.push("default".to_string());
                    } else {
                        parts.push(format!("default: {name}"));
                    }
                }
                ImportBinding::Named { imported, local } => {
                    if imported == local {
                        parts.push(imported.clone());
                    } else {
                        parts.push(format!("{imported}: {local}"));
                    }
                }
                ImportBinding::Namespace(name) => {
                    has_namespace = true;
                    ns_name = name.clone();
                }
            }
        }

        if has_namespace && !parts.is_empty() {
            dynamic_imports.push_str(&format!("const {ns_name} = await import(\"{escaped}\");\n"));
            dynamic_imports.push_str(&format!("const {{ {} }} = {ns_name};\n", parts.join(", ")));
        } else if !parts.is_empty() {
            dynamic_imports.push_str(&format!(
                "const {{ {} }} = await import(\"{escaped}\");\n",
                parts.join(", "),
            ));
        } else if has_namespace {
            dynamic_imports.push_str(&format!("const {ns_name} = await import(\"{escaped}\");\n"));
        }
    }

    // Collect loaded module metadata
    let loaded_modules: Vec<(String, String)> = graph
        .iter()
        .map(|(url, m)| (url.clone(), m.hash.clone()))
        .collect();

    Ok(BundleResult {
        dynamic_imports,
        loaded_modules,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Escape a string for embedding inside a JS double-quoted string literal.
/// Handles backslashes, double quotes, and JS line terminators.
fn js_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(ch),
        }
    }
    out
}

fn is_ident_byte(b: Option<u8>) -> bool {
    matches!(b, Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'$')
}

fn count_horizontal_ws(s: &str) -> usize {
    s.bytes().take_while(|b| *b == b' ' || *b == b'\t').count()
}

/// Skip past a string literal starting with the given quote byte.
/// Returns the number of bytes consumed (including both quotes), or None if unterminated.
fn skip_string_literal(s: &str, quote: u8) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&quote) {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2; // skip escaped character
        } else if bytes[i] == quote {
            return Some(i + 1);
        } else {
            i += 1;
        }
    }
    None // unterminated
}

/// Skip past a template literal (backtick string).
/// Handles escaped backticks but does not recurse into `${...}` expressions.
/// Returns the number of bytes consumed (including both backticks), or None if unterminated.
fn skip_template_literal(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'`') {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2; // skip escaped character
        } else if bytes[i] == b'`' {
            return Some(i + 1);
        } else {
            i += 1;
        }
    }
    None // unterminated
}

/// Parse a single import statement starting at the `import` keyword.
/// Returns `(ParsedImport, bytes_consumed)` or `None` if parsing fails.
fn parse_import_statement(s: &str) -> Option<(ParsedImport, usize)> {
    debug_assert!(s.starts_with("import"));
    let mut cur = Cursor::new(s);
    cur.advance(6); // skip "import"
    cur.skip_ws();

    let rest = cur.remaining();

    // Side-effect import: import "specifier" / import 'specifier'
    if rest.starts_with('"') || rest.starts_with('\'') {
        let spec = cur.parse_string()?;
        cur.skip_ws();
        cur.eat(b';');
        return Some((
            ParsedImport {
                specifier: spec,
                bindings: vec![],
            },
            cur.pos,
        ));
    }

    let mut bindings = Vec::new();

    if rest.starts_with('{') {
        // Named imports: import { a, b as c } from "..."
        bindings.extend(cur.parse_named_imports()?);
    } else if rest.starts_with('*') {
        // Namespace import: import * as Name from "..."
        bindings.push(ImportBinding::Namespace(cur.parse_namespace()?));
    } else {
        // Default import: import Default from "..."
        // Optionally followed by , { ... } or , * as ...
        let name = cur.parse_ident()?;
        bindings.push(ImportBinding::Default(name));
        cur.skip_ws();

        if cur.eat(b',') {
            cur.skip_ws();
            let rest2 = cur.remaining();
            if rest2.starts_with('{') {
                bindings.extend(cur.parse_named_imports()?);
            } else if rest2.starts_with('*') {
                bindings.push(ImportBinding::Namespace(cur.parse_namespace()?));
            }
        }
    }

    // Expect `from`
    cur.skip_ws();
    if !cur.remaining().starts_with("from") {
        return None;
    }
    cur.advance(4);
    cur.skip_ws();

    let specifier = cur.parse_string()?;
    cur.skip_ws();
    cur.eat(b';');

    Some((
        ParsedImport {
            specifier,
            bindings,
        },
        cur.pos,
    ))
}

/// Simple cursor for parsing import statement text.
struct Cursor<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn remaining(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    fn skip_ws(&mut self) {
        let r = self.remaining();
        self.pos += r.len() - r.trim_start().len();
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.src.as_bytes().get(self.pos) == Some(&byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Parse a quoted string (single or double) and return its contents.
    /// Handles escape sequences so that `\"` inside a string does not
    /// terminate the parse early.
    fn parse_string(&mut self) -> Option<String> {
        let r = self.remaining();
        let bytes = r.as_bytes();
        let quote = *bytes.first()?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2; // skip escaped character
            } else if bytes[i] == quote {
                let value = r[1..i].to_string();
                self.pos += i + 1;
                return Some(value);
            } else {
                i += 1;
            }
        }
        None // unterminated string
    }

    /// Parse a JavaScript identifier.
    fn parse_ident(&mut self) -> Option<String> {
        let r = self.remaining();
        let len = r
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'$')
            .count();
        if len == 0 {
            return None;
        }
        let ident = r[..len].to_string();
        self.pos += len;
        Some(ident)
    }

    /// Parse `{ a, b as c, d }` returning the named import bindings.
    fn parse_named_imports(&mut self) -> Option<Vec<ImportBinding>> {
        if !self.eat(b'{') {
            return None;
        }
        let mut bindings = Vec::new();
        loop {
            self.skip_ws();
            if self.eat(b'}') {
                break;
            }
            let imported = self.parse_ident()?;
            self.skip_ws();

            let local = if self.remaining().starts_with("as ")
                || self.remaining().starts_with("as\t")
                || self.remaining().starts_with("as\n")
            {
                self.advance(2); // skip "as"
                self.skip_ws();
                self.parse_ident()?
            } else {
                imported.clone()
            };
            bindings.push(ImportBinding::Named { imported, local });

            self.skip_ws();
            if !self.eat(b',') {
                self.skip_ws();
                self.eat(b'}');
                break;
            }
        }
        Some(bindings)
    }

    /// Parse `* as Name` and return the local name.
    fn parse_namespace(&mut self) -> Option<String> {
        if !self.eat(b'*') {
            return None;
        }
        self.skip_ws();
        // Expect "as" followed by whitespace (same boundary check as parse_named_imports)
        let r = self.remaining();
        if !(r.starts_with("as ") || r.starts_with("as\t") || r.starts_with("as\n")) {
            return None;
        }
        self.advance(2);
        self.skip_ws();
        self.parse_ident()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_import() {
        let code = "import { z } from \"zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
        assert_eq!(
            result.imports[0].bindings,
            vec![ImportBinding::Named {
                imported: "z".into(),
                local: "z".into()
            }]
        );
        assert_eq!(result.code, "async function main() {}");
    }

    #[test]
    fn multiple_named_imports() {
        let code =
            "import { encode, decode } from \"cbor-x@1.5.9/+esm\";\n\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "cbor-x@1.5.9/+esm");
        assert_eq!(
            result.imports[0].bindings,
            vec![
                ImportBinding::Named {
                    imported: "encode".into(),
                    local: "encode".into()
                },
                ImportBinding::Named {
                    imported: "decode".into(),
                    local: "decode".into()
                },
            ]
        );
        assert_eq!(result.code, "\nasync function main() {}");
    }

    #[test]
    fn renamed_import() {
        let code = "import { foo as bar } from \"pkg@1.0.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(
            result.imports[0].bindings,
            vec![ImportBinding::Named {
                imported: "foo".into(),
                local: "bar".into()
            }]
        );
    }

    #[test]
    fn default_import() {
        let code = "import Ajv from \"ajv@8.12.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "ajv@8.12.0/+esm");
        assert_eq!(
            result.imports[0].bindings,
            vec![ImportBinding::Default("Ajv".into())]
        );
    }

    #[test]
    fn namespace_import() {
        let code = "import * as Mod from \"pkg@1.0.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(
            result.imports[0].bindings,
            vec![ImportBinding::Namespace("Mod".into())]
        );
    }

    #[test]
    fn default_and_named() {
        let code =
            "import Default, { a, b as c } from \"pkg@1.0.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(
            result.imports[0].bindings,
            vec![
                ImportBinding::Default("Default".into()),
                ImportBinding::Named {
                    imported: "a".into(),
                    local: "a".into()
                },
                ImportBinding::Named {
                    imported: "b".into(),
                    local: "c".into()
                },
            ]
        );
    }

    #[test]
    fn side_effect_import() {
        let code = "import \"side-effect@1.0.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "side-effect@1.0.0/+esm");
        assert!(result.imports[0].bindings.is_empty());
    }

    #[test]
    fn single_quoted_specifier() {
        let code = "import { a } from 'pkg@1.0.0/+esm';\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports[0].specifier, "pkg@1.0.0/+esm");
    }

    #[test]
    fn multi_line_named_imports() {
        let code =
            "import {\n  a,\n  b,\n  c\n} from \"pkg@1.0.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(
            result.imports[0].bindings,
            vec![
                ImportBinding::Named {
                    imported: "a".into(),
                    local: "a".into()
                },
                ImportBinding::Named {
                    imported: "b".into(),
                    local: "b".into()
                },
                ImportBinding::Named {
                    imported: "c".into(),
                    local: "c".into()
                },
            ]
        );
    }

    #[test]
    fn multiple_imports() {
        let code = "\
import { z } from \"zod@3.22.4/+esm\";
import { format } from \"date-fns@3.6.0/+esm\";

async function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 2);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
        assert_eq!(result.imports[1].specifier, "date-fns@3.6.0/+esm");
        assert_eq!(result.code, "\nasync function main() {}");
    }

    #[test]
    fn inline_integrity_hash() {
        let code = "import { z } from \"zod@3.22.4/+esm#sha384-abc123\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm#sha384-abc123");
    }

    #[test]
    fn full_cdn_url() {
        let code = "import { z } from \"https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(
            result.imports[0].specifier,
            "https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm"
        );
    }

    #[test]
    fn no_imports() {
        let code = "async function main() { return 42; }";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    #[test]
    fn code_before_imports_preserved() {
        let code = "// comment\nimport { z } from \"zod@3.22.4/+esm\";\n\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.code, "// comment\n\nasync function main() {}");
    }

    #[test]
    fn imports_after_main_not_touched() {
        let code = "async function main() {\n  // import { z } from \"zod@3.22.4/+esm\";\n}";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    #[test]
    fn no_semicolon() {
        let code = "import { z } from \"zod@3.22.4/+esm\"\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
    }

    #[test]
    fn generate_named_dynamic_import() {
        let imports = vec![ParsedImport {
            specifier: "zod@3.22.4/+esm".into(),
            bindings: vec![ImportBinding::Named {
                imported: "z".into(),
                local: "z".into(),
            }],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(code, "const { z } = await import(\"zod@3.22.4/+esm\");\n");
    }

    #[test]
    fn generate_default_dynamic_import() {
        let imports = vec![ParsedImport {
            specifier: "ajv@8.12.0/+esm".into(),
            bindings: vec![ImportBinding::Default("Ajv".into())],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(
            code,
            "const { default: Ajv } = await import(\"ajv@8.12.0/+esm\");\n"
        );
    }

    #[test]
    fn generate_namespace_dynamic_import() {
        let imports = vec![ParsedImport {
            specifier: "pkg@1.0.0/+esm".into(),
            bindings: vec![ImportBinding::Namespace("Mod".into())],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(code, "const Mod = await import(\"pkg@1.0.0/+esm\");\n");
    }

    #[test]
    fn generate_side_effect_dynamic_import() {
        let imports = vec![ParsedImport {
            specifier: "side-effect@1.0.0/+esm".into(),
            bindings: vec![],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(code, "await import(\"side-effect@1.0.0/+esm\");\n");
    }

    #[test]
    fn generate_renamed_dynamic_import() {
        let imports = vec![ParsedImport {
            specifier: "pkg@1.0.0/+esm".into(),
            bindings: vec![ImportBinding::Named {
                imported: "foo".into(),
                local: "bar".into(),
            }],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(
            code,
            "const { foo: bar } = await import(\"pkg@1.0.0/+esm\");\n"
        );
    }

    #[test]
    fn generate_default_and_named() {
        let imports = vec![ParsedImport {
            specifier: "pkg@1.0.0/+esm".into(),
            bindings: vec![
                ImportBinding::Default("D".into()),
                ImportBinding::Named {
                    imported: "a".into(),
                    local: "a".into(),
                },
            ],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(
            code,
            "const { default: D, a } = await import(\"pkg@1.0.0/+esm\");\n"
        );
    }

    // -----------------------------------------------------------------------
    // Comment / string / template literal awareness
    // -----------------------------------------------------------------------

    #[test]
    fn import_in_block_comment_not_rewritten() {
        let code = "/*\nimport { z } from \"zod@3.22.4/+esm\";\n*/\nasync function main() {}";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    #[test]
    fn import_in_single_line_comment_not_rewritten() {
        let code = "// import { z } from \"zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    #[test]
    fn import_in_template_literal_not_rewritten() {
        let code =
            "const s = `\nimport { z } from \"zod@3.22.4/+esm\";\n`;\nasync function main() {}";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    #[test]
    fn import_in_double_quoted_string_not_rewritten() {
        let code = "const s = \"import { z } from 'zod'\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    // -----------------------------------------------------------------------
    // CDN import scanning (scan_cdn_imports)
    // -----------------------------------------------------------------------

    #[test]
    fn scan_minified_named_import() {
        let source = r#"import{hmac as t}from"/npm/@noble/hashes@1.3.2/hmac/+esm";"#;
        let results = scan_cdn_imports(source);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "/npm/@noble/hashes@1.3.2/hmac/+esm");
    }

    #[test]
    fn scan_multiple_imports() {
        let source =
            r#"import{a}from"/npm/pkg-a@1.0.0/+esm";import{b}from"/npm/pkg-b@2.0.0/+esm";"#;
        let results = scan_cdn_imports(source);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].1, "/npm/pkg-a@1.0.0/+esm");
        assert_eq!(results[1].1, "/npm/pkg-b@2.0.0/+esm");
    }

    #[test]
    fn scan_export_from() {
        let source = r#"export{hmac}from"/npm/@noble/hashes@1.3.2/hmac/+esm";"#;
        let results = scan_cdn_imports(source);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "/npm/@noble/hashes@1.3.2/hmac/+esm");
    }

    #[test]
    fn scan_side_effect_import() {
        let source = r#"import"/npm/polyfill@1.0.0/+esm";"#;
        let results = scan_cdn_imports(source);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "/npm/polyfill@1.0.0/+esm");
    }

    #[test]
    fn scan_skips_string_content() {
        // `from"/npm/..."` inside a string should not be detected
        let source = r#"var s="from\"/npm/fake@1.0.0/+esm\"";"#;
        let results = scan_cdn_imports(source);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn scan_skips_block_comment() {
        let source = r#"/* import{a}from"/npm/pkg@1.0.0/+esm"; */ var x = 1;"#;
        let results = scan_cdn_imports(source);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn scan_returns_byte_ranges() {
        let source = r#"import{a}from"/npm/pkg@1.0.0/+esm";"#;
        let results = scan_cdn_imports(source);
        assert_eq!(results.len(), 1);
        let (range, spec) = &results[0];
        // The range should point to the string content inside the quotes
        assert_eq!(&source[range.clone()], spec.as_str());
    }

    #[test]
    fn scan_single_quoted() {
        let source = "import{a}from'/npm/pkg@1.0.0/+esm';";
        let results = scan_cdn_imports(source);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "/npm/pkg@1.0.0/+esm");
    }

    #[test]
    fn scan_with_spaces() {
        let source = r#"import { a } from "/npm/pkg@1.0.0/+esm";"#;
        let results = scan_cdn_imports(source);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "/npm/pkg@1.0.0/+esm");
    }

    #[test]
    fn scan_relative_import() {
        let source = r#"import{a}from"./utils.js";"#;
        let results = scan_cdn_imports(source);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "./utils.js");
    }

    // -----------------------------------------------------------------------
    // resolve_cdn_specifier
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_root_relative_npm() {
        let result = resolve_cdn_specifier(
            "/npm/@noble/hashes@1.3.2/hmac/+esm",
            "https://cdn.jsdelivr.net/npm/ethers@6.13.4/+esm",
        );
        assert_eq!(
            result,
            Some("https://cdn.jsdelivr.net/npm/@noble/hashes@1.3.2/hmac/+esm".to_string())
        );
    }

    #[test]
    fn resolve_relative_path() {
        let result = resolve_cdn_specifier(
            "./utils.js",
            "https://cdn.jsdelivr.net/npm/pkg@1.0.0/lib/index.js",
        );
        assert_eq!(
            result,
            Some("https://cdn.jsdelivr.net/npm/pkg@1.0.0/lib/utils.js".to_string())
        );
    }

    #[test]
    fn resolve_full_url_passthrough() {
        let result = resolve_cdn_specifier("https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm", "");
        assert_eq!(
            result,
            Some("https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm".to_string())
        );
    }

    #[test]
    fn resolve_npm_specifier() {
        let result = resolve_cdn_specifier("zod@3.22.4", "");
        assert_eq!(
            result,
            Some("https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm".to_string())
        );
    }

    #[test]
    fn resolve_rejects_non_npm() {
        let result = resolve_cdn_specifier("/gh/user/repo@main/file.js", "");
        assert_eq!(result, None);
    }

    // -----------------------------------------------------------------------
    // Comment / string / template literal awareness (existing tests below)
    // -----------------------------------------------------------------------

    #[test]
    fn real_import_after_block_comment() {
        let code =
            "/* comment */\nimport { z } from \"zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
    }

    #[test]
    fn async_function_main_in_comment_not_matched() {
        let code = "// async function main\nimport { z } from \"zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
    }

    #[test]
    fn async_function_main_in_string_not_matched() {
        let code = "const s = \"async function main\";\nimport { z } from \"zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
    }
}
