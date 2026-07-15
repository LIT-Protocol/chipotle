//! The `lit.json` bundle manifest, matching gvisor-server's format so the
//! same bundle that runs in the sandbox runs under `lit run` locally.

use std::collections::BTreeMap;

use anyhow::{Result, ensure};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    /// What to run: an argv array executed verbatim, or a string treated as
    /// a shell script path (run as `sh <path>`).
    pub entrypoint: Entrypoint,
    /// Informational runtime the bundle targets (e.g. "python3"); not
    /// enforced locally.
    #[serde(default)]
    pub runtime: Option<String>,
    /// Extra environment variables for the entrypoint.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
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
