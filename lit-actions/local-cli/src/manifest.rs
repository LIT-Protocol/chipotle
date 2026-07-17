//! The optional `lit.json` bundle manifest, matching gvisor-server's format
//! so the same bundle that runs in the sandbox runs under `lit run` locally.
//! Like the sandbox, what runs is always `bash startup.sh` — the manifest
//! carries metadata only, and a legacy `entrypoint` field is ignored.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Manifest {
    /// Informational runtime the bundle targets (e.g. "python3"); not
    /// enforced locally.
    #[serde(default)]
    pub runtime: Option<String>,
    /// Extra environment variables for the startup script.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}
