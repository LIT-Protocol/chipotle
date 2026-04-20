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

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use deno_core::ModuleSpecifier;
use futures::future::try_join_all;
use swc_bundler::{Bundler, Config, Hook, Load, ModuleData, ModuleRecord, ModuleType, Resolve};
use swc_common::{
    FileName, GLOBALS, Globals, SourceMapper, Span, Spanned, SyntaxContext, sync::Lrc,
};
use swc_ecma_ast::{
    CallExpr, Callee, EsVersion, Expr, ExprOrSpread, Ident, IdentName, ImportDecl, ImportPhase,
    ImportSpecifier, ImportStarAsSpecifier, KeyValueProp, Lit, MemberExpr, MemberProp, Module,
    ModuleDecl, ModuleItem, Str,
};
use swc_ecma_codegen::{Emitter, text_writer::JsWriter};
use swc_ecma_loader::resolve::Resolution;
use swc_ecma_parser::{EsSyntax, Syntax, parse_file_as_module};
use swc_ecma_visit::{VisitMut, VisitMutWith};
use tracing::{debug, info, instrument};

use crate::cdn_module_loader::{CdnModuleLoader, MAX_MODULE_COUNT};
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
    // Pre-bundle pass: rewrite literal `import("...")` calls into static
    // `import * as __litDyn_<i> from "..."` + `Promise.resolve(__litDyn_<i>)`
    // so swc_bundler will inline the module and the runtime never re-enters
    // the module loader for them. Non-literal `import(expr)` calls are
    // rejected here because the bundler-only design (CPL-262/CPL-264) cannot
    // pre-fetch unknown specifiers at cache time.
    let (rewritten_entry, dynamic_specifiers) = rewrite_literal_dynamic_imports(user_code)
        .context("scanning entry for literal dynamic imports")?;

    let initial_urls: Vec<String> = imports
        .iter()
        .map(|imp| {
            resolve_entry_specifier(&imp.specifier)
                .with_context(|| format!("invalid import specifier: {}", imp.specifier))
        })
        .chain(dynamic_specifiers.iter().map(|spec| {
            resolve_entry_specifier(spec)
                .with_context(|| format!("invalid dynamic import specifier: {spec}"))
        }))
        .collect::<Result<_>>()?;

    info!(
        static_imports = imports.len(),
        dynamic_imports = dynamic_specifiers.len(),
        initial_urls = initial_urls.len(),
        "bundler: walking CDN dependency graph"
    );
    let walk_started = Instant::now();
    let sources = walk_deps(loader, &initial_urls).await?;
    info!(
        total_modules = sources.len(),
        walk_elapsed_ms = walk_started.elapsed().as_millis() as u64,
        "bundler: dependency graph walk complete"
    );

    tokio::task::spawn_blocking(move || run_swc_bundler(rewritten_entry, sources))
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
/// fetching + verifying each module via `loader`. Fetches within a single BFS
/// layer run concurrently via `try_join_all`, so cold-path latency scales with
/// graph depth instead of total module count.
async fn walk_deps(
    loader: &CdnModuleLoader,
    initial: &[String],
) -> Result<HashMap<String, String>> {
    let mut sources: HashMap<String, String> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut frontier: Vec<String> = Vec::new();

    for url in initial {
        if seen.insert(url.clone()) {
            frontier.push(url.clone());
        }
    }

    while !frontier.is_empty() {
        // Enforce the per-execution module cap here, before launching the
        // layer. The same check inside `fetch_module_bytes` reads
        // `loaded_modules.len()` and acts on it, which is racy under
        // `try_join_all`: every concurrent fetch in a wide layer sees the
        // pre-layer count and passes, so a single layer can blow past the
        // limit. `seen` already counts every URL we have committed to
        // fetching, so bounding it here gives a deterministic gate.
        if seen.len() > MAX_MODULE_COUNT {
            bail!("dependency graph exceeds maximum module count ({MAX_MODULE_COUNT})");
        }

        let layer_size = frontier.len();
        let layer_started = Instant::now();

        // The `seen` set guarantees each URL appears at most once in the
        // frontier, so the concurrent fetches cannot race against each other
        // for the same URL. `try_join_all` short-circuits on the first error,
        // matching the serial implementation's fail-fast behavior.
        let fetched = try_join_all(frontier.iter().map(|url| async move {
            let bytes = loader
                .fetch_module_bytes(url)
                .await
                .map_err(|e| anyhow!("failed to fetch {url}: {e}"))?;
            let source = String::from_utf8(bytes)
                .with_context(|| format!("module {url} is not valid UTF-8"))?;
            anyhow::Ok((url.clone(), source))
        }))
        .await?;

        debug!(
            modules = layer_size,
            elapsed_ms = layer_started.elapsed().as_millis() as u64,
            "bundler: fetched BFS layer"
        );

        let mut next: Vec<String> = Vec::new();
        for (url, source) in fetched {
            // Strip the integrity fragment before using as a resolver base and
            // as the in-memory map key, so the bundler's resolve step produces
            // the same key shape.
            let key = url
                .split_once('#')
                .map(|(u, _)| u.to_string())
                .unwrap_or(url);

            let deps = extract_module_imports(&source)
                .with_context(|| format!("parse error while scanning imports in {key}"))?;

            for dep_spec in deps {
                let resolved = resolve_dep_specifier(&key, &dep_spec)
                    .with_context(|| format!("cannot resolve {dep_spec} from {key}"))?;
                if seen.insert(resolved.clone()) {
                    next.push(resolved);
                }
            }

            debug!(module_url = %key, bytes = source.len(), "bundler: collected module source");
            sources.insert(key, source);
        }
        frontier = next;
    }

    Ok(sources)
}

/// Rewrite literal `import("...")` calls in `source` into a static
/// `import * as __litDyn_<i> from "<spec>"` declaration plus
/// `Promise.resolve(__litDyn_<i>)` at the call site. Returns the rewritten
/// source plus the list of newly-introduced specifiers (in alias index order).
///
/// The rewrite preserves the `Promise<Namespace>` type that the original
/// `import()` call returned, so destructuring (`const { foo } = await import(..)`)
/// continues to work.
///
/// Errors if the entry contains an `import(...)` whose argument is not a string
/// literal: the bundler cannot pre-fetch unknown specifiers, and the bundler-only
/// design rejects runtime module resolution.
///
/// SEMANTIC CHANGE — eager evaluation. A literal `import("X")` originally
/// loaded and evaluated module X lazily, only when the call site executed. After
/// rewrite, X becomes a top-level static import: it is fetched, parsed, and
/// evaluated *unconditionally at script start*, even if the original `import()`
/// call site was never reached (e.g. inside a branch never taken). Side effects
/// of X's top-level code (and of any module X transitively imports) therefore
/// run earlier and always. This matches the bundler-only design (CPL-262/264):
/// every dependency a Lit Action *might* use must be known at cache-write time,
/// so lazy/conditional `import()` is not a supported part of the Lit Action JS
/// subset. Authors needing conditional execution should branch on the namespace
/// after the await rather than relying on `import()` being skipped.
///
/// NOTE: this rewrite only runs on the user's entry source, not on fetched CDN
/// modules. CDN modules that contain dynamic `import()` calls are still left to
/// fail at execution time via `CdnModuleLoader::load`. In practice jsDelivr's
/// `+esm` bundles compile dependencies down to static imports, so this gap is
/// rarely hit; if it becomes a problem, walk_deps can be extended to apply the
/// same rewrite to each fetched module.
fn rewrite_literal_dynamic_imports(source: &str) -> Result<(String, Vec<String>)> {
    let cm: Lrc<swc_common::SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom("entry-dyn-rewrite".into())),
        source.to_string(),
    );
    let mut errors = vec![];
    let mut module = parse_file_as_module(
        &fm,
        Syntax::Es(EsSyntax::default()),
        EsVersion::Es2022,
        None,
        &mut errors,
    )
    .map_err(|e| anyhow!("parse error during dynamic-import scan: {e:?}"))?;

    let mut visitor = DynamicImportRewriter::default();
    module.visit_mut_with(&mut visitor);

    if !visitor.non_literal_spans.is_empty() {
        let count = visitor.non_literal_spans.len();
        let first = visitor.non_literal_spans[0];
        let loc = cm.lookup_char_pos(first.lo);
        let snippet = cm
            .span_to_snippet(first)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "<unavailable>".to_string());
        bail!(
            "{count} dynamic import(...) call(s) with non-literal specifiers; only string-literal specifiers can be bundled at cache-write time. First offender at line {}, col {}: `{snippet}`",
            loc.line,
            loc.col_display + 1,
        );
    }

    if visitor.specifiers.is_empty() {
        return Ok((source.to_string(), Vec::new()));
    }

    let mut prepended: Vec<ModuleItem> = visitor
        .specifiers
        .iter()
        .enumerate()
        .map(|(i, spec)| {
            ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
                span: Span::default(),
                specifiers: vec![ImportSpecifier::Namespace(ImportStarAsSpecifier {
                    span: Span::default(),
                    local: Ident::new(
                        format!("__litDyn_{i}").into(),
                        Span::default(),
                        SyntaxContext::empty(),
                    ),
                })],
                src: Box::new(Str::from(spec.clone())),
                type_only: false,
                with: None,
                phase: ImportPhase::Evaluation,
            }))
        })
        .collect();
    prepended.append(&mut module.body);
    module.body = prepended;

    let rewritten = codegen_module(&cm, &module)?;
    Ok((rewritten, visitor.specifiers))
}

#[derive(Default)]
struct DynamicImportRewriter {
    /// Specifiers found, in source order. Each occurrence gets its own alias
    /// (swc_bundler dedups identical specifiers across the bundle anyway).
    specifiers: Vec<String>,
    /// Spans of `import(expr)` calls whose argument is not a string literal.
    /// We surface these as a hard error from the caller, with line/col + snippet
    /// from the first offender so the user knows where to refactor.
    non_literal_spans: Vec<Span>,
}

impl VisitMut for DynamicImportRewriter {
    fn visit_mut_call_expr(&mut self, call: &mut CallExpr) {
        call.visit_mut_children_with(self);

        if !matches!(call.callee, Callee::Import(_)) {
            return;
        }
        if call.args.len() != 1 || call.args[0].spread.is_some() {
            // Span the whole call so the snippet captures `import(...)` shape.
            self.non_literal_spans.push(call.span);
            return;
        }
        let spec = match &*call.args[0].expr {
            Expr::Lit(Lit::Str(s)) => s.value.to_string(),
            _ => {
                // Span the offending argument expression so the snippet shows
                // exactly what couldn't be evaluated at cache-write time.
                self.non_literal_spans.push(call.args[0].expr.span());
                return;
            }
        };

        let i = self.specifiers.len();
        self.specifiers.push(spec);

        let alias = Ident::new(
            format!("__litDyn_{i}").into(),
            Span::default(),
            SyntaxContext::empty(),
        );

        // Replace the CallExpr in place with: Promise.resolve(<alias>)
        *call = CallExpr {
            span: Span::default(),
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(Expr::Member(MemberExpr {
                span: Span::default(),
                obj: Box::new(Expr::Ident(Ident::new(
                    "Promise".into(),
                    Span::default(),
                    SyntaxContext::empty(),
                ))),
                prop: MemberProp::Ident(IdentName::new("resolve".into(), Span::default())),
            }))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Ident(alias)),
            }],
            type_args: None,
        };
    }
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
    let bundle_started = Instant::now();
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

        let mut bundler = Bundler::new(
            &globals,
            cm.clone(),
            loader,
            resolver,
            config,
            Box::new(NoopHook),
        );

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

        let out = codegen_module(&cm, &bundle.module)?;
        debug!(
            elapsed_ms = bundle_started.elapsed().as_millis() as u64,
            "bundler: SWC bundle + codegen complete"
        );
        Ok(out)
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
        let fm = self.cm.new_source_file(Lrc::new(file.clone()), src.clone());
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
        let url = resolve_entry_specifier("https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm").unwrap();
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

        assert!(
            !bundled.contains("import "),
            "leftover static import: {bundled}"
        );
        assert!(
            !bundled.contains("await import("),
            "leftover dynamic import: {bundled}"
        );
        assert!(!bundled.contains("export "), "leftover export: {bundled}");
        assert!(
            bundled.contains("async function main"),
            "main missing: {bundled}"
        );
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
        assert!(
            bundled.contains("async function main"),
            "main missing: {bundled}"
        );
        assert!(
            bundled.contains("41"),
            "transitive body not inlined: {bundled}"
        );
    }

    #[test]
    fn rewrite_dynamic_imports_noop_when_none_present() {
        let src = "async function main() { return 1; }";
        let (out, specs) = rewrite_literal_dynamic_imports(src).unwrap();
        assert!(specs.is_empty());
        assert_eq!(out, src);
    }

    #[test]
    fn rewrite_dynamic_imports_inlines_literal_call() {
        let src = r#"async function main() {
  const ns = await import("zod@3.22.4/+esm");
  return ns.foo;
}"#;
        let (out, specs) = rewrite_literal_dynamic_imports(src).unwrap();
        assert_eq!(specs, vec!["zod@3.22.4/+esm"]);
        assert!(
            out.contains(r#"import * as __litDyn_0 from "zod@3.22.4/+esm""#),
            "expected static import alias prepended; got: {out}"
        );
        assert!(
            out.contains("Promise.resolve(__litDyn_0)"),
            "expected dynamic call rewritten; got: {out}"
        );
        assert!(
            !out.contains(r#"import("zod"#),
            "literal import() should be gone; got: {out}"
        );
    }

    #[test]
    fn rewrite_dynamic_imports_assigns_unique_alias_per_call() {
        let src = r#"async function main() {
  const a = await import("a@1.0.0/+esm");
  const b = await import("b@1.0.0/+esm");
  return [a, b];
}"#;
        let (out, specs) = rewrite_literal_dynamic_imports(src).unwrap();
        assert_eq!(specs, vec!["a@1.0.0/+esm", "b@1.0.0/+esm"]);
        assert!(out.contains("__litDyn_0"));
        assert!(out.contains("__litDyn_1"));
    }

    #[test]
    fn rewrite_dynamic_imports_rejects_non_literal_specifier() {
        let src = r#"async function main(spec) {
  return await import(spec);
}"#;
        let err = rewrite_literal_dynamic_imports(src).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("non-literal"),
            "expected non-literal rejection; got: {msg}"
        );
        // Error must point at where to refactor: line + col + offending snippet.
        assert!(msg.contains("line 2"), "missing line info; got: {msg}");
        assert!(msg.contains("`spec`"), "missing snippet; got: {msg}");
    }

    /// End-to-end through the bundler: a dynamic `import("...")` call must be
    /// fully inlined so the output script contains no `import(` of any kind.
    #[test]
    fn run_swc_bundler_inlines_literal_dynamic_import() {
        // Drive bundle_user_code's machinery without actually fetching: build
        // the rewritten entry + sources by hand and run run_swc_bundler.
        let entry = r#"async function main() {
  const ns = await import("https://cdn.jsdelivr.net/npm/dyn@1.0.0/+esm");
  return ns.greet();
}"#;
        let (rewritten, specs) = rewrite_literal_dynamic_imports(entry).unwrap();
        assert_eq!(specs, vec!["https://cdn.jsdelivr.net/npm/dyn@1.0.0/+esm"]);

        let mut sources = HashMap::new();
        sources.insert(
            "https://cdn.jsdelivr.net/npm/dyn@1.0.0/+esm".to_string(),
            "export function greet() { return 'hi from dyn'; }".to_string(),
        );

        let bundled = run_swc_bundler(rewritten, sources).unwrap();
        assert!(
            !bundled.contains("import("),
            "leftover dynamic import: {bundled}"
        );
        assert!(!bundled.contains("import "), "leftover import: {bundled}");
        assert!(
            bundled.contains("hi from dyn"),
            "dyn body not inlined: {bundled}"
        );
        assert!(
            bundled.contains("Promise.resolve"),
            "Promise.resolve wrap missing: {bundled}"
        );
    }

    /// Build a `CdnModuleLoader` whose cache is pre-seeded with the given
    /// `(url, source)` pairs and whose integrity manifest is empty. With no
    /// expected hash, the cache-hit path in `fetch_module_bytes` returns the
    /// cached bytes without any HTTP traffic, which lets us drive `walk_deps`
    /// deterministically in unit tests.
    fn cached_loader<I: IntoIterator<Item = (String, Vec<u8>)>>(entries: I) -> CdnModuleLoader {
        use std::sync::{Arc, RwLock};

        use lit_actions_ext::bindings::LoadedModules;

        use crate::cdn_module_loader::ModuleCache;

        let cache: ModuleCache = Arc::new(RwLock::new(HashMap::new()));
        {
            let mut w = cache.write().unwrap();
            for (url, bytes) in entries {
                w.insert(url, bytes);
            }
        }
        CdnModuleLoader::with_options(
            Arc::new(RwLock::new(HashMap::new())),
            false,
            cache,
            None,
            None,
            LoadedModules::default(),
        )
    }

    /// `walk_deps` must traverse a transitive graph (entry → a → b) across
    /// multiple BFS layers and return every reachable module. This exercises
    /// the layered fetch + dedup + dep-discovery flow added in CPL-263.
    #[tokio::test]
    async fn walk_deps_collects_transitive_graph() {
        let entry_url = "https://cdn.jsdelivr.net/npm/entry@1.0.0/+esm".to_string();
        let a_url = "https://cdn.jsdelivr.net/npm/a@1.0.0/+esm".to_string();
        let b_url = "https://cdn.jsdelivr.net/npm/b@1.0.0/+esm".to_string();

        let entry_src = format!(r#"import {{ a }} from "{a_url}"; export const e = a;"#);
        let a_src = format!(r#"import {{ b }} from "{b_url}"; export const a = b + 1;"#);
        let b_src = "export const b = 41;".to_string();

        let loader = cached_loader([
            (entry_url.clone(), entry_src.into_bytes()),
            (a_url.clone(), a_src.into_bytes()),
            (b_url.clone(), b_src.into_bytes()),
        ]);

        let sources = walk_deps(&loader, std::slice::from_ref(&entry_url))
            .await
            .unwrap();

        assert_eq!(sources.len(), 3, "expected 3 modules, got {sources:?}");
        assert!(sources.contains_key(&entry_url));
        assert!(sources.contains_key(&a_url));
        assert!(sources.contains_key(&b_url));
    }

    /// The per-execution module cap must be enforced in `walk_deps` itself,
    /// not just in `fetch_module_bytes`. The loader's check is racy under
    /// `try_join_all` — every concurrent fetch in a single layer reads the
    /// pre-layer count and passes — so a wide entry must be rejected before
    /// the layer is launched.
    #[tokio::test]
    async fn walk_deps_enforces_max_module_count_on_wide_layer() {
        let mut entries = Vec::new();
        let mut initial = Vec::new();
        for i in 0..(MAX_MODULE_COUNT + 1) {
            let url = format!("https://cdn.jsdelivr.net/npm/pkg{i}@1.0.0/+esm");
            entries.push((url.clone(), b"export const x = 1;".to_vec()));
            initial.push(url);
        }
        let loader = cached_loader(entries);

        let err = walk_deps(&loader, &initial).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("maximum module count"),
            "unexpected error: {msg}"
        );
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
        assert!(
            bundled.contains("42"),
            "default body not inlined: {bundled}"
        );
    }
}
