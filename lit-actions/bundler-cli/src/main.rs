//! `lit-bundle` — local developer CLI for any-language Lit Actions.
//!
//! Three functions, matching CPL-352:
//!   - `bundle` — package a target folder (+ optional `lit.json`) into a
//!     content-addressed OCI-style tar bundle, generating a `startup.sh` when
//!     given only a binary target.
//!   - `deploy` — register a bundle's CID on a Lit Chipotle node via the API.
//!   - `run` — execute a deployed bundle; CLI params become its `jsParams`.
//!
//! The bundle format and CID derivation match lit-api-server exactly, so a CID
//! computed here is the one the server authorizes, caches, and runs under.

mod api;
mod bundle;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context as _, Result, bail};
use clap::{Args, Parser, Subcommand};

use crate::bundle::{BuiltBundle, BundleSpec};

#[derive(Debug, Parser)]
#[command(
    name = "lit-bundle",
    about = "Bundle, deploy, and run any-language Lit Actions against a Lit Chipotle node",
    version
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Package a folder into a content-addressed bundle and print its CID.
    Bundle {
        #[command(flatten)]
        build: BuildArgs,
        /// Where to write the bundle (default: `<folder>.tar.gz`, or `.tar`
        /// with --no-compress).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Register a bundle's CID on the network so your API key may run it.
    Deploy {
        #[command(flatten)]
        build: BuildArgs,
        #[command(flatten)]
        api: ApiArgs,
        /// Human-readable action name recorded on-chain.
        #[arg(long)]
        name: Option<String>,
        /// Action description recorded on-chain.
        #[arg(long, default_value = "")]
        description: String,
    },
    /// Execute a deployed bundle. `--param`/`--params-json` become its jsParams.
    Run {
        /// Folder or prebuilt bundle file to run. Omit when using --checksum
        /// to run a bundle the node already has cached.
        target: Option<PathBuf>,
        /// Run a previously-deployed bundle by its CID (no bytes re-sent).
        #[arg(long, conflicts_with = "target")]
        checksum: Option<String>,
        /// Optional `lit.json` to include when building from a folder.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Binary to run when the folder has no startup.sh (generates one).
        #[arg(long)]
        binary: Option<String>,
        /// Emit a plain tar instead of tar.gz when building from a folder.
        #[arg(long)]
        no_compress: bool,
        /// Startup script to send with the request (overrides the bundle's own
        /// startup.sh for this run). `-` reads stdin; a path reads that file.
        #[arg(long)]
        startup_script: Option<String>,
        #[command(flatten)]
        params: ParamArgs,
        #[command(flatten)]
        api: ApiArgs,
    },
}

/// Shared inputs for building a bundle from a folder.
#[derive(Debug, Args)]
struct BuildArgs {
    /// Target folder to package.
    dir: PathBuf,
    /// Optional `lit.json` manifest (overrides any lit.json in the folder).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Binary to run when the folder has no startup.sh: a startup.sh that
    /// execs this file is generated. Required when the folder has no
    /// startup.sh (a lit.json is metadata only and does not substitute).
    #[arg(long)]
    binary: Option<String>,
    /// Emit a plain tar instead of tar.gz.
    #[arg(long)]
    no_compress: bool,
}

#[derive(Debug, Args)]
struct ApiArgs {
    /// Base URL of the Lit Chipotle node.
    #[arg(long, env = "LIT_API_URL", default_value = "http://localhost:8080")]
    api_url: String,
    /// Account or usage API key.
    #[arg(long, env = "LIT_API_KEY")]
    api_key: String,
}

#[derive(Debug, Args)]
struct ParamArgs {
    /// A jsParam as `key=value` (string). Repeatable. Overrides --params-json.
    #[arg(long = "param", value_name = "KEY=VALUE")]
    params: Vec<String>,
    /// jsParams as a full JSON object.
    #[arg(long, value_name = "JSON")]
    params_json: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Bundle { build, output } => cmd_bundle(build, output),
        Cmd::Deploy {
            build,
            api,
            name,
            description,
        } => cmd_deploy(build, api, name, description),
        Cmd::Run {
            target,
            checksum,
            config,
            binary,
            no_compress,
            startup_script,
            params,
            api,
        } => cmd_run(
            target,
            checksum,
            config,
            binary,
            no_compress,
            startup_script,
            params,
            api,
        ),
    }
}

fn build_from_args(build: &BuildArgs) -> Result<BuiltBundle> {
    if let Some(bin) = &build.binary {
        bundle::validate_binary_name(bin)?;
    }
    bundle::build(&BundleSpec {
        dir: build.dir.clone(),
        config: build.config.clone(),
        binary: build.binary.clone(),
        compress: !build.no_compress,
    })
}

fn cmd_bundle(build: BuildArgs, output: Option<PathBuf>) -> Result<()> {
    let built = build_from_args(&build)?;
    let out = output.unwrap_or_else(|| default_output(&build.dir, !build.no_compress));
    std::fs::write(&out, &built.bytes)
        .with_context(|| format!("failed to write bundle to `{}`", out.display()))?;

    if built.generated_startup {
        eprintln!("generated startup.sh from --binary");
    }
    eprintln!("wrote {} ({} bytes)", out.display(), built.bytes.len());
    // The CID goes to stdout so it can be captured in scripts.
    println!("{}", built.checksum);
    Ok(())
}

fn cmd_deploy(
    build: BuildArgs,
    api: ApiArgs,
    name: Option<String>,
    description: String,
) -> Result<()> {
    // Deploy only needs the CID to register; bundle bytes are re-sent on the
    // first run (the node caches them under this CID thereafter).
    let (_bytes, checksum) = load_or_build(build.dir.clone(), &build)?;

    let name = name.unwrap_or_else(|| {
        build
            .dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| checksum.clone())
    });

    let client = api::Client::new(&api.api_url, &api.api_key)?;
    let resp = client.add_action(&checksum, &name, &description)?;

    eprintln!("registered `{name}` on {}", api.api_url);
    eprintln!("{}", serde_json::to_string_pretty(&resp)?);
    eprintln!("run it with:  lit-bundle run --checksum {checksum} --param key=value");
    println!("{checksum}");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_run(
    target: Option<PathBuf>,
    checksum: Option<String>,
    config: Option<PathBuf>,
    binary: Option<String>,
    no_compress: bool,
    startup_script: Option<String>,
    params: ParamArgs,
    api: ApiArgs,
) -> Result<()> {
    let js_params = build_js_params(&params)?;
    let script = match startup_script {
        Some(s) => Some(read_value_or_file(&s)?),
        None => None,
    };

    let client = api::Client::new(&api.api_url, &api.api_key)?;

    let resp = if let Some(cid) = checksum {
        // Reference a cached bundle by CID; no bytes re-sent.
        client.lit_binary_action(None, Some(&cid), script.as_deref(), js_params)?
    } else {
        let target = target.ok_or_else(|| {
            anyhow::anyhow!("provide a folder/bundle file to run, or --checksum <cid>")
        })?;
        let build = BuildArgs {
            dir: target.clone(),
            config,
            binary,
            no_compress,
        };
        let (bytes, _cid) = load_or_build(target, &build)?;
        client.lit_binary_action(Some(&bytes), None, script.as_deref(), js_params)?
    };

    // Response body to stdout (capturable), logs to stderr.
    if let Some(logs) = resp.get("logs").and_then(|l| l.as_str())
        && !logs.is_empty()
    {
        eprintln!("--- logs ---\n{logs}");
    }
    let body = resp.get("response").cloned().unwrap_or(resp);
    println!("{}", serde_json::to_string_pretty(&body)?);
    Ok(())
}

/// Resolve a run/deploy target to `(bytes, checksum)`: build it if it's a
/// folder, or read + hash it if it's a prebuilt bundle file.
fn load_or_build(path: PathBuf, build: &BuildArgs) -> Result<(Vec<u8>, String)> {
    if path.is_dir() {
        let built = build_from_args(build)?;
        if built.generated_startup {
            eprintln!("generated startup.sh from --binary");
        }
        Ok((built.bytes, built.checksum))
    } else if path.is_file() {
        let bytes = std::fs::read(&path)
            .with_context(|| format!("failed to read bundle `{}`", path.display()))?;
        let checksum = bundle::checksum(&bytes);
        Ok((bytes, checksum))
    } else {
        bail!("target `{}` is not a folder or file", path.display());
    }
}

fn default_output(dir: &std::path::Path, compress: bool) -> PathBuf {
    let stem = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "bundle".to_string());
    let ext = if compress { "tar.gz" } else { "tar" };
    PathBuf::from(format!("{stem}.{ext}"))
}

/// Merge `--params-json` (base) with `--param key=value` (string overrides).
fn build_js_params(params: &ParamArgs) -> Result<serde_json::Value> {
    let mut base = match &params.params_json {
        Some(json) => serde_json::from_str::<serde_json::Value>(json)
            .context("--params-json is not valid JSON")?,
        None => serde_json::json!({}),
    };
    if !base.is_object() {
        bail!("--params-json must be a JSON object");
    }
    let obj = base.as_object_mut().expect("checked object above");
    for pair in &params.params {
        let (k, v) = pair
            .split_once('=')
            .with_context(|| format!("--param `{pair}` must be key=value"))?;
        obj.insert(k.to_string(), serde_json::Value::String(v.to_string()));
    }
    Ok(base)
}

/// `-` reads stdin; otherwise treat the value as a file path and read it.
fn read_value_or_file(value: &str) -> Result<String> {
    use std::io::Read as _;
    if value == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("failed to read startup script from stdin")?;
        Ok(buf)
    } else {
        std::fs::read_to_string(value)
            .with_context(|| format!("failed to read startup script `{value}`"))
    }
}
