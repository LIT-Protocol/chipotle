//! Guest-side `lit` CLI: exposes the Lit op-loop to action code written in
//! any language. Preinstalled in the sandbox base image; user code shells
//! out (or a language SDK wraps it) to talk to the supervisor over the
//! per-execution op socket.
//!
//! Conventions:
//! - results are printed to stdout with a trailing newline;
//! - op failures (including errors reported by lit-api-server) go to stderr
//!   and exit non-zero;
//! - `-` or an omitted value argument reads from stdin, so large payloads
//!   avoid the argv limit.

use std::io::Read as _;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use lit_actions_gvisor_server::oploop::*;
use lit_actions_gvisor_server::proto::{GetJobRequest, GuestOpsClient, Job};
use lit_actions_gvisor_server::sandbox::{ENV_OP_SOCK, GUEST_SOCK_DIR, OP_SOCK_FILE};
use lit_actions_gvisor_server::unix;
use tonic::transport::Channel;

#[derive(Debug, Parser)]
#[command(
    name = "lit",
    about = "Lit Actions guest CLI — call Lit ops from any language"
)]
struct Cli {
    /// Op socket path (mounted into the sandbox by the supervisor).
    #[arg(long, env = ENV_OP_SOCK, default_value_t = format!("{GUEST_SOCK_DIR}/{OP_SOCK_FILE}"))]
    socket: String,

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
    /// Record the action's response (returned to the caller when the
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
            Ok(buf.trim_end_matches('\n').to_string())
        }
    }
}

fn bytes_to_json(bytes: Option<Vec<u8>>) -> serde_json::Value {
    bytes
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(serde_json::Value::Null)
}

async fn get_job(client: &mut GuestOpsClient<Channel>) -> Result<Job> {
    Ok(client.get_job(GetJobRequest {}).await?.into_inner())
}

/// The action's own CID: explicit argument if given, else from the job.
async fn own_ipfs_id(
    client: &mut GuestOpsClient<Channel>,
    ipfs_id: Option<String>,
) -> Result<String> {
    match ipfs_id {
        Some(id) => Ok(id),
        None => Ok(get_job(client).await?.ipfs_id),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let channel = unix::connect_to_socket(PathBuf::from(&cli.socket))
        .await
        .with_context(|| format!("failed to connect to op socket {}", cli.socket))?;
    let mut client = GuestOpsClient::new(channel);

    match cli.cmd {
        Cmd::Job => {
            let job = get_job(&mut client).await?;
            let json = serde_json::json!({
                "ipfsId": job.ipfs_id,
                "timeoutMs": job.timeout_ms,
                "httpHeaders": job.http_headers,
                "jsParams": bytes_to_json(job.js_params),
                "authContext": bytes_to_json(job.auth_context),
            });
            println!("{json:#}");
        }
        Cmd::Params => {
            let job = get_job(&mut client).await?;
            println!("{}", bytes_to_json(job.js_params));
        }
        Cmd::AuthContext => {
            let job = get_job(&mut client).await?;
            println!("{}", bytes_to_json(job.auth_context));
        }
        Cmd::Print { message } => {
            let message = value_or_stdin(message)?;
            client
                .print(PrintRequest {
                    message: format!("{message}\n"),
                })
                .await?;
        }
        Cmd::SetResponse { response } => {
            let response = value_or_stdin(response)?;
            client.set_response(SetResponseRequest { response }).await?;
        }
        Cmd::GetPrivateKey { pkp_id } => {
            let resp = client
                .get_private_key(GetPrivateKeyRequest { pkp_id })
                .await?;
            println!("{}", resp.into_inner().secret);
        }
        Cmd::GetActionPrivateKey => {
            let resp = client
                .get_lit_action_private_key(GetLitActionPrivateKeyRequest {})
                .await?;
            println!("{}", resp.into_inner().secret);
        }
        Cmd::GetActionPublicKey { ipfs_id } => {
            let ipfs_id = own_ipfs_id(&mut client, ipfs_id).await?;
            let resp = client
                .get_lit_action_public_key(GetLitActionPublicKeyRequest { ipfs_id })
                .await?;
            println!("{}", resp.into_inner().public_key);
        }
        Cmd::GetActionWalletAddress { ipfs_id } => {
            let ipfs_id = own_ipfs_id(&mut client, ipfs_id).await?;
            let resp = client
                .get_lit_action_wallet_address(GetLitActionWalletAddressRequest { ipfs_id })
                .await?;
            println!("{}", resp.into_inner().wallet_address);
        }
        Cmd::AesEncrypt { pkp_id, message } => {
            let message = value_or_stdin(message)?;
            let resp = client
                .aes_encrypt(AesEncryptRequest { pkp_id, message })
                .await?;
            println!("{}", resp.into_inner().ciphertext);
        }
        Cmd::AesDecrypt { pkp_id, ciphertext } => {
            let ciphertext = value_or_stdin(ciphertext)?;
            let resp = client
                .aes_decrypt(AesDecryptRequest { pkp_id, ciphertext })
                .await?;
            println!("{}", resp.into_inner().plaintext);
        }
        Cmd::IncrementFetchCount => {
            let resp = client
                .increment_fetch_count(IncrementFetchCountRequest {})
                .await?;
            println!("{}", resp.into_inner().fetch_count);
        }
    }

    Ok(())
}
