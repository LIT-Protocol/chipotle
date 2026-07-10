//! Language registry for multi-language Lit Actions (CPL-349).
//!
//! One registry drives both the `GET /get_supported_languages` discovery
//! endpoint and request admission: it is the source of truth for "what does
//! this node run". Which `(language, runtime, method)` tuples a node actually
//! advertises is deployment config — the `LIT_SUPPORTED_LANGUAGES` env var —
//! validated at startup against the built-in table of known languages, so a
//! typo in deploy config fails the boot instead of advertising a language the
//! node can't run.
//!
//! Allowlist grammar (canonical form; entries `;`-separated):
//!
//! ```text
//! name[:runtime[:runtime...]]|method[,method...]
//! ```
//!
//! e.g. `javascript|raw_script; python:python3.13:python3.12|raw_script,bundle; rust|bundle`
//!
//! The first runtime listed for a language is its default. Omitting the
//! runtime list for a runtime-bearing language (e.g. `python|raw_script`)
//! enables all runtimes this build knows about, with the first as default;
//! list them explicitly to restrict to a subset. Repeated `|` are tolerated
//! on parse (`javascript||raw_script` == `javascript|raw_script`).

use anyhow::{Context, Result, anyhow, bail};
use rocket_okapi::okapi::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Env var holding the node's language allowlist, set by the deploy pipeline.
/// Unset or blank means the pre-multi-language surface: JavaScript only.
pub const LIT_SUPPORTED_LANGUAGES_ENV: &str = "LIT_SUPPORTED_LANGUAGES";

/// One supported language, as advertised by `GET /get_supported_languages`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LanguageFeature {
    /// Stable id used in requests, e.g. "python", "rust", "javascript".
    pub name: String,
    /// Human label, e.g. "Python".
    pub display_name: String,
    /// Underlying runner: "deno" (JS) or "gvisor" (everything else).
    pub execution_model: ExecutionModel,
    /// Provisionable runtime versions — each maps to an install recipe and a
    /// cache profile (NOT baked into the image). Multiple may coexist
    /// (e.g. 3.12 and 3.13). Empty for compiled/static languages.
    pub runtimes: Vec<LanguageRuntime>,
    /// Which methods this language accepts on this node.
    pub methods: Vec<ExecutionMethod>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LanguageRuntime {
    /// Value clients pass as `runtime` and the manifest's `runtime` field,
    /// e.g. "python3.13". Selects an install recipe + cache profile.
    pub id: String,
    /// Full version string, e.g. "3.13.1".
    pub version: String,
    /// Chosen when the client omits `runtime`.
    pub is_default: bool,
    /// True once this profile's install layers are materialized in the
    /// gVisor runner's cache. Always false until the install cache lands
    /// (CPL-349 phase 2); pre-warm status is wired in phase 5.
    pub prewarmed: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMethod {
    /// Pick language + runtime, send code; server desugars into a bundle.
    RawScript,
    /// Code-only tar over the minimal base rootfs; runtime provisioned by the
    /// startup script's cached install commands (or a static binary).
    Bundle,
    /// Self-contained OCI image bringing its own runtime/interpreter.
    /// Node-gated; may be curated in prod.
    OciBundle,
}

impl ExecutionMethod {
    fn parse(token: &str) -> Result<Self> {
        match token {
            "raw_script" => Ok(Self::RawScript),
            "bundle" => Ok(Self::Bundle),
            "oci_bundle" => Ok(Self::OciBundle),
            other => bail!("unknown execution method {other:?}"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::RawScript => "raw_script",
            Self::Bundle => "bundle",
            Self::OciBundle => "oci_bundle",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionModel {
    Deno,
    Gvisor,
}

/// A language this build knows how to run, independent of whether this
/// node's deploy config enables it.
struct KnownLanguage {
    name: &'static str,
    display_name: &'static str,
    execution_model: ExecutionModel,
    /// Runtime ids and versions the registry has install recipes for.
    runtimes: &'static [(&'static str, &'static str)],
    methods: &'static [ExecutionMethod],
}

/// Everything the codebase can run. `LIT_SUPPORTED_LANGUAGES` selects a
/// subset of this table; anything outside it is rejected at startup.
const KNOWN_LANGUAGES: &[KnownLanguage] = &[
    KnownLanguage {
        name: "javascript",
        display_name: "JavaScript",
        execution_model: ExecutionModel::Deno,
        runtimes: &[],
        methods: &[ExecutionMethod::RawScript],
    },
    KnownLanguage {
        name: "python",
        display_name: "Python",
        execution_model: ExecutionModel::Gvisor,
        runtimes: &[("python3.13", "3.13.1"), ("python3.12", "3.12.7")],
        methods: &[
            ExecutionMethod::RawScript,
            ExecutionMethod::Bundle,
            ExecutionMethod::OciBundle,
        ],
    },
    KnownLanguage {
        name: "rust",
        display_name: "Rust (compiled binary)",
        execution_model: ExecutionModel::Gvisor,
        runtimes: &[],
        methods: &[ExecutionMethod::Bundle, ExecutionMethod::OciBundle],
    },
];

/// The parsed, validated language allowlist — Rocket managed state, built
/// once at startup from `LIT_SUPPORTED_LANGUAGES`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportedLanguages {
    languages: Vec<LanguageFeature>,
}

impl SupportedLanguages {
    /// Reads `LIT_SUPPORTED_LANGUAGES`. Unset or blank falls back to the
    /// pre-multi-language surface (JavaScript only); a set-but-invalid value
    /// is an error so a bad deploy config fails the boot.
    pub fn from_env() -> Result<Self> {
        match std::env::var(LIT_SUPPORTED_LANGUAGES_ENV) {
            Ok(raw) if !raw.trim().is_empty() => Self::parse(&raw)
                .with_context(|| format!("invalid {LIT_SUPPORTED_LANGUAGES_ENV}: {raw:?}")),
            _ => Self::parse(Self::DEFAULT),
        }
    }

    /// The surface advertised when the env var is absent: what every node
    /// runs today, JS on Deno via `/lit_action`.
    pub const DEFAULT: &'static str = "javascript|raw_script";

    pub fn parse(allowlist: &str) -> Result<Self> {
        let mut languages: Vec<LanguageFeature> = Vec::new();

        for entry in allowlist.split(';') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }

            let mut fields = entry.split('|').map(str::trim).filter(|f| !f.is_empty());
            let spec = fields
                .next()
                .ok_or_else(|| anyhow!("entry {entry:?} is missing a language"))?;
            let methods_field = fields
                .next()
                .ok_or_else(|| anyhow!("entry {entry:?} is missing methods"))?;
            if fields.next().is_some() {
                bail!("entry {entry:?} has more than two '|'-separated fields");
            }

            let mut spec_tokens = spec.split(':').map(str::trim);
            let name = spec_tokens.next().unwrap_or_default();
            let known = KNOWN_LANGUAGES
                .iter()
                .find(|k| k.name == name)
                .ok_or_else(|| anyhow!("unknown language {name:?}"))?;
            if languages.iter().any(|l| l.name == known.name) {
                bail!("language {name:?} listed more than once");
            }

            let mut runtimes: Vec<LanguageRuntime> = Vec::new();
            for id in spec_tokens.filter(|t| !t.is_empty()) {
                let (_, version) = known
                    .runtimes
                    .iter()
                    .find(|(known_id, _)| *known_id == id)
                    .ok_or_else(|| anyhow!("unknown runtime {id:?} for language {name:?}"))?;
                if runtimes.iter().any(|r| r.id == id) {
                    bail!("runtime {id:?} listed more than once for language {name:?}");
                }
                runtimes.push(LanguageRuntime {
                    id: id.to_string(),
                    version: version.to_string(),
                    is_default: runtimes.is_empty(),
                    prewarmed: false,
                });
            }
            // A runtime-bearing language listed with no explicit runtimes
            // (e.g. `python|raw_script`) means "all runtimes this build knows"
            // — otherwise we'd advertise the language with an empty `runtimes`
            // array and no default for clients that omit `runtime`. Restrict to
            // a subset (what prod does) by listing them explicitly. Languages
            // with no known runtimes (js, rust) correctly stay empty.
            if runtimes.is_empty() {
                runtimes = known
                    .runtimes
                    .iter()
                    .enumerate()
                    .map(|(i, (id, version))| LanguageRuntime {
                        id: id.to_string(),
                        version: version.to_string(),
                        is_default: i == 0,
                        prewarmed: false,
                    })
                    .collect();
            }

            let mut methods: Vec<ExecutionMethod> = Vec::new();
            for token in methods_field.split(',').map(str::trim) {
                let method =
                    ExecutionMethod::parse(token).with_context(|| format!("in entry {entry:?}"))?;
                if !known.methods.contains(&method) {
                    bail!("language {name:?} does not support method {token:?}");
                }
                if methods.contains(&method) {
                    bail!("method {token:?} listed more than once for language {name:?}");
                }
                methods.push(method);
            }
            if methods.is_empty() {
                bail!("entry {entry:?} lists no methods");
            }

            languages.push(LanguageFeature {
                name: known.name.to_string(),
                display_name: known.display_name.to_string(),
                execution_model: known.execution_model,
                runtimes,
                methods,
            });
        }

        if languages.is_empty() {
            bail!("allowlist {allowlist:?} enables no languages");
        }

        Ok(Self { languages })
    }

    pub fn languages(&self) -> &[LanguageFeature] {
        &self.languages
    }

    /// Whether this node admits `(language, method)`; drives request
    /// admission for `/lit_raw_action` and `/lit_binary_action`.
    pub fn allows(&self, language: &str, method: ExecutionMethod) -> bool {
        self.languages
            .iter()
            .any(|l| l.name == language && l.methods.contains(&method))
    }
}

impl std::fmt::Display for SupportedLanguages {
    /// Canonical allowlist encoding; `parse` round-trips it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, lang) in self.languages.iter().enumerate() {
            if i > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{}", lang.name)?;
            for runtime in &lang.runtimes {
                write!(f, ":{}", runtime.id)?;
            }
            write!(f, "|")?;
            for (j, method) in lang.methods.iter().enumerate() {
                if j > 0 {
                    write!(f, ",")?;
                }
                write!(f, "{}", method.as_str())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_planned_next_allowlist() {
        let parsed = SupportedLanguages::parse(
            "javascript||raw_script; python:python3.13:python3.12|raw_script,bundle; rust||bundle",
        )
        .unwrap();

        let langs = parsed.languages();
        assert_eq!(langs.len(), 3);

        assert_eq!(langs[0].name, "javascript");
        assert_eq!(langs[0].execution_model, ExecutionModel::Deno);
        assert!(langs[0].runtimes.is_empty());
        assert_eq!(langs[0].methods, vec![ExecutionMethod::RawScript]);

        assert_eq!(langs[1].name, "python");
        assert_eq!(langs[1].execution_model, ExecutionModel::Gvisor);
        assert_eq!(langs[1].runtimes.len(), 2);
        assert_eq!(langs[1].runtimes[0].id, "python3.13");
        assert_eq!(langs[1].runtimes[0].version, "3.13.1");
        assert!(langs[1].runtimes[0].is_default);
        assert!(!langs[1].runtimes[1].is_default);
        assert_eq!(
            langs[1].methods,
            vec![ExecutionMethod::RawScript, ExecutionMethod::Bundle]
        );

        assert_eq!(langs[2].name, "rust");
        assert_eq!(langs[2].methods, vec![ExecutionMethod::Bundle]);
    }

    #[test]
    fn canonical_form_round_trips() {
        let canonical =
            "javascript|raw_script; python:python3.13:python3.12|raw_script,bundle; rust|bundle";
        let parsed = SupportedLanguages::parse(canonical).unwrap();
        assert_eq!(parsed.to_string(), canonical);
        assert_eq!(
            SupportedLanguages::parse(&parsed.to_string()).unwrap(),
            parsed
        );
    }

    #[test]
    fn default_is_javascript_only() {
        let parsed = SupportedLanguages::parse(SupportedLanguages::DEFAULT).unwrap();
        assert_eq!(parsed.languages().len(), 1);
        assert_eq!(parsed.languages()[0].name, "javascript");
        assert_eq!(parsed.to_string(), SupportedLanguages::DEFAULT);
    }

    #[test]
    fn rejects_unknown_language() {
        let err = SupportedLanguages::parse("cobol|bundle").unwrap_err();
        assert!(err.to_string().contains("unknown language"), "{err}");
    }

    #[test]
    fn rejects_unknown_runtime() {
        let err = SupportedLanguages::parse("python:python2.7|bundle").unwrap_err();
        assert!(err.to_string().contains("unknown runtime"), "{err}");
    }

    #[test]
    fn runtime_bearing_language_without_listed_runtimes_gets_all_known() {
        // `python|...` with no runtime list must not advertise an empty
        // `runtimes` (and no default) — it enables every known runtime.
        let parsed = SupportedLanguages::parse("python|raw_script,bundle").unwrap();
        let python = &parsed.languages()[0];
        assert_eq!(python.runtimes.len(), 2);
        assert_eq!(python.runtimes[0].id, "python3.13");
        assert!(python.runtimes[0].is_default);
        assert_eq!(python.runtimes[1].id, "python3.12");
        assert!(!python.runtimes[1].is_default);
    }

    #[test]
    fn runtime_less_language_without_runtimes_stays_empty() {
        let parsed = SupportedLanguages::parse("rust|bundle").unwrap();
        assert!(parsed.languages()[0].runtimes.is_empty());
    }

    #[test]
    fn rejects_runtime_for_language_without_runtimes() {
        let err = SupportedLanguages::parse("rust:rust1.80|bundle").unwrap_err();
        assert!(err.to_string().contains("unknown runtime"), "{err}");
    }

    #[test]
    fn rejects_unknown_method() {
        let err = SupportedLanguages::parse("python|wasm").unwrap_err();
        let chain = format!("{err:#}");
        assert!(chain.contains("unknown execution method"), "{chain}");
    }

    #[test]
    fn rejects_method_the_language_does_not_support() {
        let err = SupportedLanguages::parse("javascript|bundle").unwrap_err();
        assert!(err.to_string().contains("does not support"), "{err}");
    }

    #[test]
    fn rejects_duplicate_language() {
        let err = SupportedLanguages::parse("python|bundle; python|raw_script").unwrap_err();
        assert!(err.to_string().contains("more than once"), "{err}");
    }

    #[test]
    fn rejects_empty_allowlist() {
        let err = SupportedLanguages::parse(" ; ").unwrap_err();
        assert!(err.to_string().contains("enables no languages"), "{err}");
    }

    #[test]
    fn rejects_entry_without_methods() {
        let err = SupportedLanguages::parse("python").unwrap_err();
        assert!(err.to_string().contains("missing methods"), "{err}");
    }

    #[test]
    fn allows_checks_language_and_method() {
        let parsed =
            SupportedLanguages::parse("python:python3.13|raw_script,bundle; rust|bundle").unwrap();
        assert!(parsed.allows("python", ExecutionMethod::RawScript));
        assert!(parsed.allows("rust", ExecutionMethod::Bundle));
        assert!(!parsed.allows("rust", ExecutionMethod::RawScript));
        assert!(!parsed.allows("javascript", ExecutionMethod::RawScript));
    }
}
