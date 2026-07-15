//! Local `lit` CLI — a laptop stand-in for the sandbox guest `lit` CLI from
//! the any-language runner (gvisor-server, PR #557).
//!
//! The guest CLI proxies each op over a Unix socket to lit-api-server, which
//! holds the real keys in the TEE. This one implements the same ops
//! *locally*: keys are derived from a developer-supplied master key, `print`
//! / `set-response` write to a local state dir, and `params` / `job` come
//! from a local job file. The command surface is identical, so an action
//! developed and tested here runs unchanged in the TEE.
//!
//! Conventions match the guest CLI: results go to stdout with a trailing
//! newline; failures go to stderr and exit non-zero; a `-` or omitted value
//! argument is read from stdin.

mod crypto;
mod hexutil;
mod job;
mod keys;
mod manifest;
mod state;

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};

use crate::job::{DEFAULT_IPFS_ID, Job};
use crate::state::State;

/// Env var the guest CLI reads for the action's content id; honored here too
/// so `lit run` and a real sandbox set the same variable.
const ENV_ACTION_IPFS_ID: &str = "LIT_ACTION_IPFS_ID";
const DEFAULT_JOB_FILE: &str = "lit.job.json";

#[derive(Debug, Parser)]
#[command(
    name = "lit",
    about = "Lit Actions local CLI — run and test any-language Lit Actions with keys derived from a local private key",
    version
)]
struct Cli {
    /// 32-byte master private key (hex, `0x`-optional) all local keys derive
    /// from. If unset, a key is generated and cached in the state dir.
    #[arg(long, env = "LIT_LOCAL_PRIVATE_KEY", global = true)]
    key: Option<String>,

    /// Directory holding per-run local state (master key, response, logs,
    /// fetch counter).
    #[arg(
        long,
        env = "LIT_LOCAL_STATE_DIR",
        default_value = ".lit-local",
        global = true
    )]
    state_dir: PathBuf,

    /// Job file supplying params / authContext / headers (defaults to
    /// `lit.job.json` in the working directory when present).
    #[arg(long, env = "LIT_LOCAL_JOB", global = true)]
    job: Option<PathBuf>,

    /// This action's content id (the local analog of the server-derived
    /// CID). Overrides any `ipfsId` in the job file.
    #[arg(long, env = ENV_ACTION_IPFS_ID, global = true)]
    ipfs_id: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Print the full job (params, auth context, headers, cid) as JSON
    Job,
    /// Print the job's jsParams JSON (null if none were supplied)
    Params,
    /// Print the job's authContext JSON (null if none was supplied)
    AuthContext,
    /// Append a message to the action's log output
    Print {
        /// Message text; omit or pass `-` to read from stdin
        message: Option<String>,
    },
    /// Record the action's response (surfaced by `lit run` when the
    /// entrypoint exits)
    SetResponse {
        /// Response text; omit or pass `-` to read from stdin
        response: Option<String>,
    },
    /// Fetch the raw derived private key for a PKP wallet
    GetPrivateKey { pkp_id: String },
    /// Fetch this action's own derived private key
    GetActionPrivateKey,
    /// Fetch the public key of an action (defaults to this action)
    GetActionPublicKey { ipfs_id: Option<String> },
    /// Fetch the wallet address of an action (defaults to this action)
    GetActionWalletAddress { ipfs_id: Option<String> },
    /// AES-encrypt a message with a PKP-derived key
    AesEncrypt {
        pkp_id: String,
        /// Plaintext; omit or pass `-` to read from stdin
        message: Option<String>,
    },
    /// AES-decrypt a ciphertext with a PKP-derived key
    AesDecrypt {
        pkp_id: String,
        /// Ciphertext; omit or pass `-` to read from stdin
        ciphertext: Option<String>,
    },
    /// Increment the action's HTTP fetch counter; prints the new count
    IncrementFetchCount,
    /// Local-only: run a bundle's entrypoint with `lit` on PATH and the job
    /// wired up, then print the recorded response (mirrors the supervisor).
    Run {
        /// Path to the bundle manifest.
        #[arg(long, default_value = "lit.json")]
        manifest: PathBuf,
        /// Keep response/logs/counter from a previous run instead of
        /// clearing them first.
        #[arg(long)]
        keep_state: bool,
    },
}

/// Resolve an optional value argument, treating `None` and `-` as stdin.
fn value_or_stdin(value: Option<String>) -> Result<String> {
    match value {
        Some(v) if v != "-" => Ok(v),
        _ => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("failed to read value from stdin")?;
            // Strip a trailing newline (LF or CRLF) so piped values match
            // what was typed; CRLF files/pipes on Windows would otherwise
            // leave a stray `\r` in ciphertext/plaintext.
            Ok(buf.trim_end_matches(['\r', '\n']).to_string())
        }
    }
}

/// Locate the job file: explicit flag/env, else `lit.job.json` in `base_dir`
/// if present. For `lit run`, `base_dir` is the bundle directory so job
/// discovery follows the action, not the parent process's CWD.
fn resolve_job_path(explicit: Option<PathBuf>, base_dir: &Path) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p);
    }
    let default = base_dir.join(DEFAULT_JOB_FILE);
    default.exists().then_some(default)
}

/// Load the job, discovering `lit.job.json` under `base_dir` when no `--job`
/// was given, then applying the `--ipfs-id` override.
fn load_job(cli: &Cli, base_dir: &Path) -> Result<Job> {
    let job_path = resolve_job_path(cli.job.clone(), base_dir);
    let mut job = Job::load(job_path.as_deref(), DEFAULT_IPFS_ID)?;
    if let Some(id) = &cli.ipfs_id {
        job.ipfs_id = id.clone();
    }
    Ok(job)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("lit: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let state = State::open(&cli.state_dir)?;

    // `run` orchestrates child processes and resolves its job relative to the
    // bundle directory (see cmd_run); every other command reads the job from
    // the current working directory and needs the master key up front.
    if let Cmd::Run {
        manifest,
        keep_state,
    } = &cli.cmd
    {
        return cmd_run(&cli, &state, manifest, *keep_state);
    }

    let job = load_job(&cli, Path::new("."))?;
    // Resolve the master key lazily: read-only commands (job/params/…) must
    // not generate or touch a key just to run.
    let key_arg = cli.key.clone();
    let master = || state.master_key(key_arg.as_deref());

    match cli.cmd {
        Cmd::Job => println!("{:#}", job.to_json()),
        Cmd::Params => println!("{}", job.js_params),
        Cmd::AuthContext => println!("{}", job.auth_context),
        Cmd::Print { message } => {
            let message = value_or_stdin(message)?;
            let line = format!("{message}\n");
            state.append_log(&line)?;
            // Local visibility analog of the op-loop log: mirror to stderr so
            // the developer sees it without polluting captured stdout.
            eprint!("{line}");
        }
        Cmd::SetResponse { response } => {
            let response = value_or_stdin(response)?;
            state.set_response(&response)?;
            eprintln!("lit: recorded response ({} bytes)", response.len());
        }
        Cmd::GetPrivateKey { pkp_id } => {
            let secret = keys::pkp_secret(&master()?, &pkp_id);
            println!("{}", hexutil::bytes_to_0x_hex(&secret));
        }
        Cmd::GetActionPrivateKey => {
            let secret = keys::action_secret(&master()?, &job.ipfs_id);
            println!("{}", hexutil::bytes_to_0x_hex(&secret));
        }
        Cmd::GetActionPublicKey { ipfs_id } => {
            let cid = ipfs_id.unwrap_or_else(|| job.ipfs_id.clone());
            let signer = keys::action_signer(&master()?, &cid)?;
            println!(
                "{}",
                hexutil::bytes_to_0x_hex(&keys::public_key_bytes(&signer))
            );
        }
        Cmd::GetActionWalletAddress { ipfs_id } => {
            let cid = ipfs_id.unwrap_or_else(|| job.ipfs_id.clone());
            let signer = keys::action_signer(&master()?, &cid)?;
            println!("{}", hexutil::bytes_to_0x_hex(signer.address().as_slice()));
        }
        Cmd::AesEncrypt { pkp_id, message } => {
            let message = value_or_stdin(message)?;
            let key = keys::pkp_secret(&master()?, &pkp_id);
            println!("{}", crypto::aes_encrypt(&key, &message)?);
        }
        Cmd::AesDecrypt { pkp_id, ciphertext } => {
            let ciphertext = value_or_stdin(ciphertext)?;
            let key = keys::pkp_secret(&master()?, &pkp_id);
            println!("{}", crypto::aes_decrypt(&key, &ciphertext)?);
        }
        Cmd::IncrementFetchCount => {
            println!("{}", state.increment_fetch_count()?);
        }
        Cmd::Run { .. } => unreachable!("handled above"),
    }

    Ok(ExitCode::SUCCESS)
}

/// Run a bundle's entrypoint locally: prepend this binary's dir to PATH so
/// the child's `lit` calls resolve back here, wire the job/state through the
/// environment (exactly as the sandbox sets `LIT_OP_SOCK`), then surface the
/// recorded response — the local analog of the supervisor.
fn cmd_run(cli: &Cli, state: &State, manifest_path: &Path, keep_state: bool) -> Result<ExitCode> {
    let raw = std::fs::read(manifest_path)
        .with_context(|| format!("failed to read manifest {}", manifest_path.display()))?;
    let manifest: manifest::Manifest = serde_json::from_slice(&raw)
        .with_context(|| format!("manifest {} is invalid", manifest_path.display()))?;
    let argv = manifest.entrypoint.to_argv()?;

    if !keep_state {
        state.reset_run()?;
    }

    // The entrypoint runs from the manifest's directory so relative script
    // paths resolve; state/job paths handed to the child must be absolute.
    let action_dir = manifest_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

    // Discover the job next to the bundle (not the parent CWD), so the child
    // and the CID we export below agree with the action's own `lit.job.json`.
    let job_path = resolve_job_path(cli.job.clone(), &action_dir);
    let job = load_job(cli, &action_dir)?;

    // The child writes state here, so create it now and hand over an
    // absolute path (the child runs in the bundle dir, not this CWD).
    state.ensure_dir()?;
    let abs_state_dir = std::fs::canonicalize(&cli.state_dir)
        .with_context(|| format!("failed to resolve state dir {}", cli.state_dir.display()))?;

    // Prepend this binary's dir so the child's `lit` calls resolve back here,
    // preserving any existing PATH portably (Windows uses `;`, not `:`).
    let self_dir = std::env::current_exe()
        .context("failed to locate current executable")?
        .parent()
        .context("executable has no parent dir")?
        .to_path_buf();
    let mut path_entries = vec![self_dir];
    if let Some(existing) = std::env::var_os("PATH") {
        path_entries.extend(std::env::split_paths(&existing));
    }
    let path_env = std::env::join_paths(path_entries).context("failed to build child PATH")?;

    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]).current_dir(&action_dir);
    // Apply the manifest's env FIRST so none of the CLI's wiring below can be
    // clobbered by a bundle that sets PATH / LIT_LOCAL_STATE_DIR / etc.
    // (parity with "the sandbox controls the environment").
    for (k, v) in &manifest.env {
        cmd.env(k, v);
    }
    cmd.env("PATH", path_env)
        .env("LIT_LOCAL_STATE_DIR", &abs_state_dir)
        .env(ENV_ACTION_IPFS_ID, &job.ipfs_id);
    if let Some(key) = &cli.key {
        cmd.env("LIT_LOCAL_PRIVATE_KEY", key);
    }
    if let Some(job_path) = job_path {
        let abs = std::fs::canonicalize(&job_path)
            .with_context(|| format!("failed to resolve job file {}", job_path.display()))?;
        cmd.env("LIT_LOCAL_JOB", abs);
    }

    let runtime = manifest.runtime.as_deref().unwrap_or("unspecified");
    eprintln!(
        "lit: running {argv:?} (cid {}, runtime {runtime})",
        job.ipfs_id
    );
    let status = cmd
        .status()
        .with_context(|| format!("failed to spawn entrypoint {argv:?}"))?;

    if status.success() {
        // Serverless semantics: only a clean exit returns the recorded
        // response. On failure we must NOT print it — a caller reading stdout
        // would otherwise mistake a failed run for a successful one.
        match state.response()? {
            Some(response) => {
                eprintln!("lit: --- recorded response ---");
                println!("{response}");
            }
            None => eprintln!("lit: (no response recorded)"),
        }
        Ok(ExitCode::SUCCESS)
    } else {
        let code = status.code().unwrap_or(1);
        eprintln!("lit: action exited with status {status} (no response returned)");
        Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)))
    }
}
