//! Pre-execution bundler for lit-action code.
//!
//! Walks the jsDelivr CDN dependency graph, fetches and integrity-verifies each
//! module, and uses `swc_bundler` to emit a single self-contained JavaScript
//! string with all `import` statements fully inlined. The output is safe to
//! execute via `v8::Isolate::execute_script` with no further module resolution
//! — every `await import(...)` call is eliminated.
//!
//! This is the "resolve right down" form referenced in CPL-262: the cached
//! rewrite no longer defers module loading to Deno's `ModuleLoader`, so the
//! internal `resolve`/`load` pipeline is not re-entered on every action run.

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{Context, Result, anyhow, bail};
use deno_core::ModuleSpecifier;
use swc_bundler::{Bundler, Config, Hook, Load, ModuleData, ModuleRecord, ModuleType, Resolve};
use swc_common::{FileName, GLOBALS, Globals, Span, SyntaxContext, sync::Lrc};
use swc_ecma_ast::{EsVersion, KeyValueProp, Module, ModuleDecl, ModuleItem};
use swc_ecma_codegen::{Emitter, text_writer::JsWriter};
use swc_ecma_loader::resolve::Resolution;
use swc_ecma_parser::{EsSyntax, Syntax, parse_file_as_module};
use tracing::{debug, info, instrument};

use crate::cdn_module_loader::CdnModuleLoader;
use crate::import_rewriter::ParsedImport;

/// Synthetic FileName used for the user's entry point. Chosen to be distinct
/// from any real URL so it cannot collide with a fetched module.
pub(crate) const SYNTHETIC_ENTRY_URL: &str = "lit-action:entry.js";

/// Bundle the user's entry point and all transitive CDN imports into a single
/// self-contained JS string. The output contains no `import`/`import()` calls.
///
/// - `user_code`: the raw user code, including its static `import` statements
/// - `imports`: parsed imports (used to seed the dep-graph walk)
/// - `loader`: used to fetch + integrity-verify each CDN module
#[instrument(skip_all, err, fields(imports = imports.len()))]
pub(crate) async fn bundle_user_code(
    user_code: &str,
    imports: &[ParsedImport],
    loader: &CdnModuleLoader,
) -> Result<String> {
    let initial_urls: Vec<String> = imports
        .iter()
        .map(|imp| {
            resolve_entry_specifier(&imp.specifier)
                .with_context(|| format!("invalid import specifier: {}", imp.specifier))
        })
        .collect::<Result<_>>()?;

    info!(
        initial_imports = initial_urls.len(),
        "bundler: walking CDN dependency graph"
    );
    let sources = walk_deps(loader, &initial_urls).await?;
    info!(
        total_modules = sources.len(),
        "bundler: dependency graph walk complete"
    );

    let entry_src = user_code.to_string();
    tokio::task::spawn_blocking(move || run_swc_bundler(entry_src, sources))
        .await
        .context("bundler task join failed")?
}

/// Resolve a user-facing entry-point import specifier to an absolute CDN URL.
fn resolve_entry_specifier(spec: &str) -> Result<String> {
    if spec.starts_with("https://") {
        if !CdnModuleLoader::is_allowed_cdn(spec) {
            bail!("URL {spec} is not on the allowed jsDelivr npm CDN");
        }
        return Ok(spec.to_string());
    }
    CdnModuleLoader::parse_npm_specifier(spec)
        .ok_or_else(|| anyhow!("cannot resolve specifier {spec}"))
}

/// Resolve a dependency specifier found inside a fetched module to an absolute
/// CDN URL, relative to the URL it was imported from.
fn resolve_dep_specifier(base_url: &str, spec: &str) -> Result<String> {
    // jsDelivr's +esm bundles reference sibling packages via either relative
    // paths (`./util.js`) or absolute origin-relative paths (`/npm/other@1/+esm`).
    // Both are resolved against the base URL using standard URL joining.
    if spec.starts_with("./") || spec.starts_with("../") || spec.starts_with('/') {
        let base = ModuleSpecifier::parse(base_url)
            .with_context(|| format!("invalid base URL {base_url}"))?;
        let joined = base
            .join(spec)
            .with_context(|| format!("cannot join {spec} against {base_url}"))?;
        let s = joined.to_string();
        if !CdnModuleLoader::is_allowed_cdn(&s) {
            bail!("resolved URL {s} is outside the allowed jsDelivr npm path");
        }
        return Ok(s);
    }
    if spec.starts_with("https://") {
        if !CdnModuleLoader::is_allowed_cdn(spec) {
            bail!("URL {spec} is not on the allowed jsDelivr npm CDN");
        }
        return Ok(spec.to_string());
    }
    CdnModuleLoader::parse_npm_specifier(spec)
        .ok_or_else(|| anyhow!("cannot parse specifier {spec} (from {base_url})"))
}

/// Walk the dependency graph from the initial entry's imports outward,
/// fetching + verifying each module via `loader`. Returns a map of
/// absolute URL → module source.
async fn walk_deps(
    loader: &CdnModuleLoader,
    initial: &[String],
) -> Result<HashMap<String, String>> {
    let mut sources: HashMap<String, String> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    for url in initial {
        if seen.insert(url.clone()) {
            queue.push_back(url.clone());
        }
    }

    while let Some(url) = queue.pop_front() {
        let bytes = loader
            .fetch_module_bytes(&url)
            .await
            .map_err(|e| anyhow!("failed to fetch {url}: {e}"))?;
        let source = String::from_utf8(bytes)
            .with_context(|| format!("module {url} is not valid UTF-8"))?;

        // Strip the integrity fragment before using as a resolver base and
        // as the in-memory map key, so the bundler's resolve step produces
        // the same key shape.
        let key = url
            .split_once('#')
            .map(|(u, _)| u.to_string())
            .unwrap_or_else(|| url.clone());

        let deps = extract_module_imports(&source)
            .with_context(|| format!("parse error while scanning imports in {key}"))?;

        for dep_spec in deps {
            let resolved = resolve_dep_specifier(&key, &dep_spec)
                .with_context(|| format!("cannot resolve {dep_spec} from {key}"))?;
            if seen.insert(resolved.clone()) {
                queue.push_back(resolved);
            }
        }

        debug!(module_url = %key, bytes = source.len(), "bundler: collected module source");
        sources.insert(key, source);
    }

    Ok(sources)
}

/// Parse a JS source and return every static import/re-export specifier.
/// Dynamic `import()` calls are intentionally not followed. Any surviving
/// dynamic import is rejected by `CdnModuleLoader::load` at execution time,
/// which is the runtime safety net for CPL-262.
fn extract_module_imports(source: &str) -> Result<Vec<String>> {
    let cm: Lrc<swc_common::SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom("dep-scan".into())),
        source.to_string(),
    );
    let mut errors = vec![];
    let module = parse_file_as_module(
        &fm,
        Syntax::Es(EsSyntax::default()),
        EsVersion::Es2022,
        None,
        &mut errors,
    )
    .map_err(|e| anyhow!("parse error: {e:?}"))?;

    let mut out = Vec::new();
    for item in &module.body {
        if let ModuleItem::ModuleDecl(decl) = item {
            match decl {
                ModuleDecl::Import(i) => out.push(i.src.value.to_string()),
                ModuleDecl::ExportAll(e) => out.push(e.src.value.to_string()),
                ModuleDecl::ExportNamed(e) => {
                    if let Some(src) = e.src.as_ref() {
                        out.push(src.value.to_string());
                    }
                }
                _ => {}
            }
        }
    }
    Ok(out)
}

fn run_swc_bundler(entry_src: String, mut sources: HashMap<String, String>) -> Result<String> {
    // The user's entry is a script that defines `async function main`; it has
    // no exports. But swc_bundler's DCE treats an entry with no exports as a
    // module with no live definitions and drops everything. Append a synthetic
    // `export { main }` so DCE keeps `main` (and transitively the deps it
    // uses). We strip the export back out post-bundle in `strip_module_decls`.
    let entry_with_export = format!("{entry_src}\n;export {{ main }};\n");

    // Insert the synthetic entry last so we cannot accidentally overwrite
    // a real CDN source of the same name (not possible in practice since
    // the scheme is disjoint, but defend anyway).
    sources.insert(SYNTHETIC_ENTRY_URL.to_string(), entry_with_export);

    let cm: Lrc<swc_common::SourceMap> = Default::default();
    let globals = Globals::default();
    GLOBALS.set(&globals, || -> Result<String> {
        let loader = InMemoryLoad {
            cm: cm.clone(),
            sources,
        };
        let resolver = InMemoryResolve;
        let config = Config {
            require: false,
            disable_inliner: false,
            disable_hygiene: false,
            disable_fixer: false,
            disable_dce: false,
            external_modules: vec![],
            module: ModuleType::Es,
        };

        let mut bundler =
            Bundler::new(&globals, cm.clone(), loader, resolver, config, Box::new(NoopHook));

        let mut entries: HashMap<String, FileName> = HashMap::new();
        entries.insert(
            "entry".to_string(),
            FileName::Custom(SYNTHETIC_ENTRY_URL.to_string()),
        );

        let bundles = bundler
            .bundle(entries)
            .map_err(|e| anyhow!("swc_bundler failed: {e:?}"))?;

        let mut bundle = bundles
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("swc_bundler returned no bundles"))?;

        // Drop any leftover top-level `export` / `import` declarations. We're
        // emitting script-compatible code, so these are invalid; exports from
        // the entry are unused (the executor reaches `main` via lexical scope)
        // and any surviving imports would mean a dependency was not inlined —
        // which is a bundler bug we want to surface early.
        strip_module_decls(&mut bundle.module)?;

        codegen_module(&cm, &bundle.module)
    })
}

/// Remove top-level `import`/`export` declarations from the bundle so the
/// output parses as a script (no ESM syntax). Fails if a static import
/// survived bundling — that would indicate a dependency was not inlined.
fn strip_module_decls(module: &mut Module) -> Result<()> {
    let mut kept = Vec::with_capacity(module.body.len());
    for item in module.body.drain(..) {
        match item {
            ModuleItem::ModuleDecl(ModuleDecl::Import(i)) => {
                bail!(
                    "bundler produced an unresolved import of {:?}; expected every dependency to be inlined",
                    i.src.value
                );
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(e)) => {
                // Keep the inner declaration, drop the export wrapper.
                kept.push(ModuleItem::Stmt(swc_ecma_ast::Stmt::Decl(e.decl)));
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(_))
            | ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(_))
            | ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(_))
            | ModuleItem::ModuleDecl(ModuleDecl::ExportAll(_))
            | ModuleItem::ModuleDecl(ModuleDecl::TsImportEquals(_))
            | ModuleItem::ModuleDecl(ModuleDecl::TsExportAssignment(_))
            | ModuleItem::ModuleDecl(ModuleDecl::TsNamespaceExport(_)) => {
                // Drop: the entry doesn't need to re-export anything.
            }
            other => kept.push(other),
        }
    }
    module.body = kept;
    Ok(())
}

fn codegen_module(cm: &Lrc<swc_common::SourceMap>, module: &Module) -> Result<String> {
    let mut buf: Vec<u8> = vec![];
    {
        let wr = JsWriter::new(cm.clone(), "\n", &mut buf, None);
        let mut emitter = Emitter {
            cfg: swc_ecma_codegen::Config::default()
                .with_target(EsVersion::Es2022)
                .with_minify(false)
                .with_ascii_only(false),
            cm: cm.clone(),
            comments: None,
            wr,
        };
        emitter
            .emit_module(module)
            .map_err(|e| anyhow!("codegen failed: {e}"))?;
    }
    String::from_utf8(buf).context("codegen produced non-UTF8 output")
}

struct InMemoryLoad {
    cm: Lrc<swc_common::SourceMap>,
    sources: HashMap<String, String>,
}

impl Load for InMemoryLoad {
    fn load(&self, file: &FileName) -> Result<ModuleData, anyhow::Error> {
        let url = match file {
            FileName::Custom(u) => u.as_str(),
            other => bail!("unexpected FileName variant in bundler Load: {other:?}"),
        };
        let src = self
            .sources
            .get(url)
            .ok_or_else(|| anyhow!("bundler requested {url} but it was not pre-fetched"))?;
        let fm = self
            .cm
            .new_source_file(Lrc::new(file.clone()), src.clone());
        let mut errors = vec![];
        let module = parse_file_as_module(
            &fm,
            Syntax::Es(EsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut errors,
        )
        .map_err(|e| anyhow!("parse error for {url}: {e:?}"))?;

        // Suppress the unused-var lint for Span/SyntaxContext imports kept for
        // clarity in this module.
        let _ = (Span::default(), SyntaxContext::empty());

        Ok(ModuleData {
            fm,
            module,
            helpers: Default::default(),
        })
    }
}

struct InMemoryResolve;

impl Resolve for InMemoryResolve {
    fn resolve(&self, base: &FileName, specifier: &str) -> Result<Resolution, anyhow::Error> {
        let base_url = match base {
            FileName::Custom(u) => u.as_str(),
            other => bail!("unexpected base FileName variant in bundler Resolve: {other:?}"),
        };
        let resolved = if base_url == SYNTHETIC_ENTRY_URL {
            resolve_entry_specifier(specifier)?
        } else {
            resolve_dep_specifier(base_url, specifier)?
        };
        Ok(Resolution {
            filename: FileName::Custom(resolved),
            slug: None,
        })
    }
}

struct NoopHook;

impl Hook for NoopHook {
    fn get_import_meta_props(
        &self,
        _span: Span,
        _record: &ModuleRecord,
    ) -> Result<Vec<KeyValueProp>, anyhow::Error> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_module_decls_removes_exports() {
        let cm: Lrc<swc_common::SourceMap> = Default::default();
        let fm = cm.new_source_file(
            Lrc::new(FileName::Custom("t.js".into())),
            "export function main(){} export default 1; export * from 'x'; function keep(){}"
                .to_string(),
        );
        let mut errors = vec![];
        let mut module = parse_file_as_module(
            &fm,
            Syntax::Es(EsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut errors,
        )
        .unwrap();
        strip_module_decls(&mut module).unwrap();
        let code = codegen_module(&cm, &module).unwrap();
        assert!(!code.contains("export"), "leftover export: {code}");
        assert!(code.contains("function main"), "main dropped: {code}");
        assert!(code.contains("function keep"), "keep dropped: {code}");
    }

    #[test]
    fn strip_module_decls_rejects_unresolved_import() {
        let cm: Lrc<swc_common::SourceMap> = Default::default();
        let fm = cm.new_source_file(
            Lrc::new(FileName::Custom("t.js".into())),
            "import { x } from 'nope';".to_string(),
        );
        let mut errors = vec![];
        let mut module = parse_file_as_module(
            &fm,
            Syntax::Es(EsSyntax::default()),
            EsVersion::Es2022,
            None,
            &mut errors,
        )
        .unwrap();
        let err = strip_module_decls(&mut module).unwrap_err();
        assert!(format!("{err:#}").contains("unresolved import"));
    }

    #[test]
    fn resolve_entry_specifier_npm() {
        let url = resolve_entry_specifier("zod@3.22.4/+esm").unwrap();
        assert_eq!(url, "https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm");
    }

    #[test]
    fn resolve_entry_specifier_full_url() {
        let url =
            resolve_entry_specifier("https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm").unwrap();
        assert_eq!(url, "https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm");
    }

    #[test]
    fn resolve_entry_specifier_rejects_other_cdn() {
        let err = resolve_entry_specifier("https://evil.example.com/foo.js").unwrap_err();
        assert!(format!("{err:#}").contains("jsDelivr"));
    }

    #[test]
    fn resolve_dep_specifier_relative() {
        let base = "https://cdn.jsdelivr.net/npm/pkg@1.0.0/dist/index.js";
        let resolved = resolve_dep_specifier(base, "./util.js").unwrap();
        assert_eq!(
            resolved,
            "https://cdn.jsdelivr.net/npm/pkg@1.0.0/dist/util.js"
        );
    }

    #[test]
    fn resolve_dep_specifier_relative_escape_rejected() {
        let base = "https://cdn.jsdelivr.net/npm/pkg@1.0.0/dist/index.js";
        let err = resolve_dep_specifier(base, "../../../gh/evil/repo@1.0.0/x.js").unwrap_err();
        assert!(format!("{err:#}").contains("outside"));
    }

    /// jsDelivr's +esm bundles reference sibling packages with origin-absolute
    /// paths like `/npm/@noble/curves@2.0.1/secp256k1.js/+esm`. Resolving these
    /// against the base URL must produce a full CDN URL.
    #[test]
    fn resolve_dep_specifier_absolute_path() {
        let base = "https://cdn.jsdelivr.net/npm/micro-eth-signer@0.18.1/+esm";
        let resolved =
            resolve_dep_specifier(base, "/npm/@noble/curves@2.0.1/secp256k1.js/+esm").unwrap();
        assert_eq!(
            resolved,
            "https://cdn.jsdelivr.net/npm/@noble/curves@2.0.1/secp256k1.js/+esm"
        );
    }

    #[test]
    fn resolve_dep_specifier_absolute_path_escape_rejected() {
        let base = "https://cdn.jsdelivr.net/npm/pkg@1.0.0/+esm";
        let err = resolve_dep_specifier(base, "/gh/evil/repo@1.0.0/x.js").unwrap_err();
        assert!(format!("{err:#}").contains("outside"));
    }

    #[test]
    fn extract_imports_includes_exports_from() {
        let src = "import { a } from 'x'; export { b } from 'y'; export * from 'z';";
        let imports = extract_module_imports(src).unwrap();
        assert_eq!(imports, vec!["x", "y", "z"]);
    }

    /// End-to-end test of the SWC bundler with a two-module in-memory graph.
    /// Verifies that named imports are inlined, the user's `main` is preserved,
    /// and no `import`/`export` keywords remain.
    #[test]
    fn run_swc_bundler_inlines_named_import() {
        let entry = r#"
            import { greet } from "https://cdn.jsdelivr.net/npm/greet@1.0.0/+esm";
            async function main() { return greet("world"); }
        "#;
        let mut sources = HashMap::new();
        sources.insert(
            "https://cdn.jsdelivr.net/npm/greet@1.0.0/+esm".to_string(),
            "export function greet(name) { return `hello ${name}`; }".to_string(),
        );

        let bundled = run_swc_bundler(entry.to_string(), sources).unwrap();

        assert!(!bundled.contains("import "), "leftover static import: {bundled}");
        assert!(
            !bundled.contains("await import("),
            "leftover dynamic import: {bundled}"
        );
        assert!(!bundled.contains("export "), "leftover export: {bundled}");
        assert!(bundled.contains("async function main"), "main missing: {bundled}");
        assert!(
            bundled.contains("hello"),
            "greet body not inlined: {bundled}"
        );
    }

    /// A transitive dep graph (entry → a → b) must be fully inlined.
    #[test]
    fn run_swc_bundler_walks_transitive_deps() {
        let entry = r#"
            import { top } from "https://cdn.jsdelivr.net/npm/a@1.0.0/+esm";
            async function main() { return top(); }
        "#;
        let mut sources = HashMap::new();
        sources.insert(
            "https://cdn.jsdelivr.net/npm/a@1.0.0/+esm".to_string(),
            r#"import { base } from "https://cdn.jsdelivr.net/npm/b@1.0.0/+esm";
               export function top() { return base() + 1; }"#
                .to_string(),
        );
        sources.insert(
            "https://cdn.jsdelivr.net/npm/b@1.0.0/+esm".to_string(),
            "export function base() { return 41; }".to_string(),
        );

        let bundled = run_swc_bundler(entry.to_string(), sources).unwrap();

        assert!(!bundled.contains("import "), "leftover import: {bundled}");
        assert!(!bundled.contains("export "), "leftover export: {bundled}");
        assert!(bundled.contains("async function main"), "main missing: {bundled}");
        assert!(bundled.contains("41"), "transitive body not inlined: {bundled}");
    }

    /// Default exports must route to a local binding usable by the entry.
    #[test]
    fn run_swc_bundler_inlines_default_export() {
        let entry = r#"
            import Def from "https://cdn.jsdelivr.net/npm/def@1.0.0/+esm";
            async function main() { return Def(); }
        "#;
        let mut sources = HashMap::new();
        sources.insert(
            "https://cdn.jsdelivr.net/npm/def@1.0.0/+esm".to_string(),
            "export default function() { return 42; }".to_string(),
        );

        let bundled = run_swc_bundler(entry.to_string(), sources).unwrap();
        assert!(!bundled.contains("import "), "leftover import: {bundled}");
        assert!(!bundled.contains("export "), "leftover export: {bundled}");
        assert!(bundled.contains("42"), "default body not inlined: {bundled}");
    }
}
