use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use deno_core::{OpState, extension, op2};
use deno_error::JsErrorBox;
use lit_actions_grpc::proto::*;
use tracing::instrument;

use crate::macros::*;

/// Per-execution tracker that records every module loaded during a single
/// Lit Action run, including its resolved CDN URL and SHA-384 hash.
/// Shared between the module loader and OpState so that `showImportDetails()`
/// can read it from user code.
#[derive(Clone, Default)]
pub struct LoadedModules(pub Arc<RwLock<Vec<LoadedModuleInfo>>>);

/// Metadata for a single loaded CDN module.
#[derive(Clone, Debug)]
pub struct LoadedModuleInfo {
    /// The resolved CDN URL (without fragment).
    pub url: String,
    /// The base64-encoded SHA-384 hash of the module content.
    pub hash: String,
}

#[instrument(skip_all, ret)]
#[op2(fast)]
fn op_print(state: &mut OpState, #[string] msg: &str, is_err: bool) -> Result<(), JsErrorBox> {
    use std::io::{IsTerminal, Write, stderr, stdout};

    lazy_static::lazy_static! {
        static ref IS_ATTY_STDOUT: bool = stdout().is_terminal();
        static ref IS_ATTY_STDERR: bool = stderr().is_terminal();
    }

    let prepended = format!("[JSEnv] {msg}");
    if is_err && *IS_ATTY_STDERR {
        stderr()
            .write_all(prepended.as_bytes())
            .and_then(|_| stderr().flush())
            .map_err(|e| {
                JsErrorBox::generic(format!("op_print: failed to write to stderr: {e}"))
            })?;
    } else if *IS_ATTY_STDOUT {
        stdout()
            .write_all(prepended.as_bytes())
            .and_then(|_| stdout().flush())
            .map_err(|e| {
                JsErrorBox::generic(format!("op_print: failed to write to stdout: {e}"))
            })?;
    }

    // Ignore Deno logs enabled by WorkerLogLevel::Debug
    if msg.starts_with("DEBUG JS") {
        return Ok(());
    }

    remote_op!(op_print,
        state,
        PrintRequest { message: msg.to_string() }, // may be empty
        UnionRequest::Print(_) => Ok(())
    )
}

// Deny use of Deno.exit, which would terminate lit-actions via std::process::exit.
// Mimics Deno Deploy's behavior of patching Deno.exit like this:
//
// function exit() {
//   throw new errors.PermissionDenied(
//     "'Deno.exit' is not allowed in this context.",
//   );
// }
#[instrument(skip_all, ret)]
#[op2(fast)]
fn op_exit(_state: &mut OpState) -> Result<(), JsErrorBox> {
    Err(JsErrorBox::new(
        "PermissionDenied",
        "'Deno.exit' is not allowed in this context.",
    ))
}

#[instrument(skip_all, ret)]
#[op2(fast)]
fn op_set_response(state: &mut OpState, #[string] response: String) -> Result<(), JsErrorBox> {
    remote_op!(op_set_response,
        state,
        SetResponseRequest { response }, // may be empty
        UnionRequest::SetResponse(_) => Ok(())
    )
}

#[instrument(skip_all, ret)]
#[op2(reentrant)]
async fn op_increment_fetch_count(state: Rc<RefCell<OpState>>) -> Result<u32, JsErrorBox> {
    remote_op_async!(op_increment_fetch_count,
        state,
        IncrementFetchCountRequest {},
        UnionRequest::IncrementFetchCount(resp) => Ok(resp.fetch_count)
    )
}

#[instrument(skip_all, ret)]
#[op2(reentrant)]
#[string]
async fn op_aes_encrypt(
    state: Rc<RefCell<OpState>>,
    #[string] pkp_id: String,
    #[string] message: String,
) -> Result<String, JsErrorBox> {
    ensure_not_blank!(pkp_id, "pkpId");
    ensure_not_blank!(message, "message");

    remote_op_async!(op_aes_encrypt,
        state,
        AesEncryptRequest { pkp_id, message },
        UnionRequest::AesEncrypt(resp) => Ok(resp.ciphertext)
    )
}

#[instrument(skip_all, ret)]
#[op2(reentrant)]
#[string]
async fn op_aes_decrypt(
    state: Rc<RefCell<OpState>>,
    #[string] pkp_id: String,
    #[string] ciphertext: String,
) -> Result<String, JsErrorBox> {
    ensure_not_blank!(pkp_id, "pkpId");
    ensure_not_empty!(ciphertext);

    remote_op_async!(op_aes_decrypt,
        state,
        AesDecryptRequest { pkp_id, ciphertext },
        UnionRequest::AesDecrypt(resp) => Ok(resp.plaintext)
    )
}

#[instrument(skip_all, ret)]
#[op2(reentrant)]
#[string]
async fn op_get_private_key(
    state: Rc<RefCell<OpState>>,
    #[string] pkp_id: String,
) -> Result<String, JsErrorBox> {
    ensure_not_blank!(pkp_id, "pkpId");

    remote_op_async!(op_get_private_key,
        state,
        GetPrivateKeyRequest { pkp_id },
        UnionRequest::GetPrivateKey(resp) => Ok(resp.secret)
    )
}

#[instrument(skip_all, ret)]
#[op2(reentrant)]
#[string]
async fn op_get_lit_action_private_key(state: Rc<RefCell<OpState>>) -> Result<String, JsErrorBox> {
    remote_op_async!(op_get_lit_action_private_key,
        state,
        GetLitActionPrivateKeyRequest {},
        UnionRequest::GetLitActionPrivateKey(resp) => Ok(resp.secret)
    )
}

#[instrument(skip_all, ret)]
#[op2(reentrant)]
#[string]
async fn op_get_lit_action_public_key(
    state: Rc<RefCell<OpState>>,
    #[string] ipfs_id: String,
) -> Result<String, JsErrorBox> {
    ensure_not_blank!(ipfs_id, "ipfsId");

    remote_op_async!(op_get_lit_action_public_key,
        state,
        GetLitActionPublicKeyRequest { ipfs_id },
        UnionRequest::GetLitActionPublicKey(resp) => Ok(resp.public_key)
    )
}

#[instrument(skip_all, ret)]
#[op2(reentrant)]
#[string]
async fn op_get_lit_action_wallet_address(
    state: Rc<RefCell<OpState>>,
    #[string] ipfs_id: String,
) -> Result<String, JsErrorBox> {
    ensure_not_blank!(ipfs_id, "ipfsId");

    remote_op_async!(op_get_lit_action_wallet_address,
        state,
        GetLitActionWalletAddressRequest { ipfs_id },
        UnionRequest::GetLitActionWalletAddress(resp) => Ok(resp.wallet_address)
    )
}

#[instrument(skip_all, ret)]
#[op2(reentrant)]
async fn op_update_resource_usage(
    state: Rc<RefCell<OpState>>,
    tick: u32,
    used_kb: u32,
) -> Result<bool, JsErrorBox> {
    remote_op_async!(op_update_resource_usage,
        state,
        UpdateResourceUsageRequest { tick, used_kb },
        UnionRequest::UpdateResourceUsage(resp) => Ok(resp.cancel_action)
    )
}

#[instrument(skip_all, ret)]
#[op2]
#[string]
fn op_show_import_details(state: &mut OpState) -> Result<String, JsErrorBox> {
    // Clone the Arc to release the borrow on OpState before calling remote_op!
    let loaded_modules: LoadedModules = state
        .try_borrow::<LoadedModules>()
        .cloned()
        .ok_or_else(|| JsErrorBox::generic("Import tracking not available"))?;

    let modules = loaded_modules
        .0
        .read()
        .map_err(|e| JsErrorBox::generic(format!("Failed to read import details: {e}")))?;

    // Build JSON array of {url, hash} objects
    let details: Vec<serde_json::Value> = modules
        .iter()
        .map(|m| {
            serde_json::json!({
                "url": &m.url,
                "hash": format!("sha384-{}", &m.hash),
            })
        })
        .collect();

    let json = serde_json::to_string_pretty(&details)
        .map_err(|e| JsErrorBox::generic(format!("Failed to serialize import details: {e}")))?;

    // Log via the existing print opCode
    remote_op!(
        op_show_import_details,
        state,
        PrintRequest {
            message: format!("[Import Details]\n{json}\n")
        },
        UnionRequest::Print(_) => Ok(json)
    )
}

#[instrument(skip_all, ret)]
pub async fn op_update_resource_usage_external(
    state: Rc<RefCell<OpState>>,
    tick: u32,
    used_kb: u32,
) -> Result<bool, JsErrorBox> {
    remote_op_async!(op_update_resource_usage,
        state,
        UpdateResourceUsageRequest { tick, used_kb },
        UnionRequest::UpdateResourceUsage(resp) => Ok(resp.cancel_action)
    )
}

const PROXIED_FETCH_TIMEOUT_SECS: u64 = 30;
const PROXIED_FETCH_MAX_BYTES: usize = 10 * 1024 * 1024; // 10 MiB

#[derive(serde::Deserialize)]
struct ProxiedFetchRequest {
    url: String,
    #[serde(default = "default_proxied_fetch_method")]
    method: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    body: Option<String>,
    /// `http(s)://[user:pass@]host:port`. Empty/None → a direct (unproxied) request.
    #[serde(default)]
    proxy: Option<String>,
}

fn default_proxied_fetch_method() -> String {
    "GET".to_string()
}

/// Split `scheme://user:pass@host:port` into (`scheme://host:port`, Some((user, pass))).
/// Userinfo is taken up to the last `@` before the host; the user/pass split is
/// on the first `:`. Returns no credentials when there is no userinfo.
fn split_proxy_credentials(proxy_url: &str) -> (String, Option<(String, String)>) {
    let Some((scheme, rest)) = proxy_url.split_once("://") else {
        return (proxy_url.to_string(), None);
    };
    let Some((userinfo, hostport)) = rest.rsplit_once('@') else {
        return (proxy_url.to_string(), None);
    };
    let (user, pass) = match userinfo.split_once(':') {
        Some((u, p)) => (u.to_string(), p.to_string()),
        None => (userinfo.to_string(), String::new()),
    };
    (format!("{scheme}://{hostport}"), Some((user, pass)))
}

#[derive(serde::Serialize)]
struct ProxiedFetchResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

/// Hard cap on distinct pooled clients; beyond it requests still work but get
/// a one-off client (no eviction churn, no unbounded growth from hostile
/// per-request proxy strings).
const PROXIED_FETCH_CLIENT_POOL_MAX: usize = 16;

lazy_static::lazy_static! {
    /// One `reqwest::Client` per proxy URL ("" = direct). `Client` is an Arc
    /// around a connection pool, so reuse keeps TCP+TLS sessions warm across
    /// calls instead of paying a fresh handshake per request (plan M2
    /// follow-up). Keys contain proxy credentials — they are secret material
    /// and must never be logged.
    static ref PROXIED_FETCH_CLIENTS: std::sync::Mutex<std::collections::HashMap<String, reqwest::Client>> =
        std::sync::Mutex::new(std::collections::HashMap::new());
}

fn proxied_fetch_client(proxy: Option<&str>) -> Result<reqwest::Client, JsErrorBox> {
    let key = proxy.unwrap_or_default();
    if let Some(client) = PROXIED_FETCH_CLIENTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(key)
    {
        return Ok(client.clone());
    }

    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(PROXIED_FETCH_TIMEOUT_SECS));
    if !key.is_empty() {
        // Pull any `user:pass@` out of the URL and apply it explicitly:
        // reqwest::Proxy does not reliably forward URL userinfo as the
        // Proxy-Authorization header, so an authenticated proxy (e.g. Webshare)
        // would otherwise stall the CONNECT until timeout.
        let (base_url, credentials) = split_proxy_credentials(key);
        let mut proxy = reqwest::Proxy::all(&base_url).map_err(|e| {
            JsErrorBox::generic(format!("op_lit_proxied_fetch: invalid proxy URL: {e}"))
        })?;
        if let Some((user, pass)) = credentials {
            proxy = proxy.basic_auth(&user, &pass);
        }
        builder = builder.proxy(proxy);
    }
    let client = builder.build().map_err(|e| {
        JsErrorBox::generic(format!("op_lit_proxied_fetch: failed to build client: {e}"))
    })?;

    let mut pool = PROXIED_FETCH_CLIENTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if pool.len() < PROXIED_FETCH_CLIENT_POOL_MAX {
        pool.insert(key.to_string(), client.clone());
    }
    Ok(client)
}

/// Outbound HTTP for a Lit Action that can egress through a per-request
/// authenticated proxy. This lets an action reach a venue (e.g. Binance) from a
/// chosen non-US IP even though the enclave's own egress is geo-blocked
/// (HTTP 451). Egress happens in-process via `reqwest` — the same place Deno's
/// `fetch` egresses from — and the proxy is reached over an HTTP CONNECT tunnel,
/// so TLS to the venue is end-to-end and the proxy never sees venue credentials
/// or payloads. The per-action fetch quota is enforced by the JS wrapper calling
/// `op_increment_fetch_count` before this op; the byte cap bounds memory the way
/// `deno_fetch` does. Unlike the other ops here it does no gRPC round-trip to
/// lit-node — the request is purely local.
//
// NB: `async(lazy)`, not bare `async`: in edition 2024 a bare `async` keyword in
// attribute position fails to parse ("expected `async(...)`"). The other async
// ops here sidestep it via the `reentrant` *identifier*; we want plain async
// semantics, and the parenthesized `async(lazy)` form parses cleanly and is a
// real (non-fake) async op. No `#[instrument]` (it re-mangles the keyword too).
#[op2(async(lazy))]
#[serde]
async fn op_lit_proxied_fetch(
    #[serde] req: ProxiedFetchRequest,
) -> Result<ProxiedFetchResponse, JsErrorBox> {
    ensure_not_blank!(req.url, "url");

    let client = proxied_fetch_client(req.proxy.as_deref().map(str::trim).filter(|p| !p.is_empty()))?;

    let method = reqwest::Method::from_bytes(req.method.trim().to_ascii_uppercase().as_bytes())
        .map_err(|e| JsErrorBox::generic(format!("op_lit_proxied_fetch: invalid method: {e}")))?;
    let mut rb = client.request(method, req.url.trim());
    for (name, value) in &req.headers {
        rb = rb.header(name.as_str(), value.as_str());
    }
    if let Some(body) = req.body {
        rb = rb.body(body);
    }

    let mut resp = rb.send().await.map_err(|e| {
        let mut chain = format!("{e}");
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            chain.push_str(&format!(" -> {s}"));
            src = s.source();
        }
        JsErrorBox::generic(format!(
            "op_lit_proxied_fetch: request failed (timeout={} connect={} request={}): {chain}",
            e.is_timeout(),
            e.is_connect(),
            e.is_request()
        ))
    })?;

    let status = resp.status().as_u16();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or_default().to_string()))
        .collect();

    // Streamed read with a hard cap so a hostile or runaway response can't blow
    // the action's 64MB memory budget.
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| {
        JsErrorBox::generic(format!("op_lit_proxied_fetch: body read failed: {e}"))
    })? {
        if buf.len() + chunk.len() > PROXIED_FETCH_MAX_BYTES {
            return Err(JsErrorBox::generic(format!(
                "op_lit_proxied_fetch: response exceeded {PROXIED_FETCH_MAX_BYTES} bytes"
            )));
        }
        buf.extend_from_slice(&chunk);
    }

    Ok(ProxiedFetchResponse {
        status,
        headers,
        body: String::from_utf8_lossy(&buf).into_owned(),
    })
}

// ---------------------------------------------------------------------------
// Email send + approval primitive (plan D6/M3).
//
// sendEmail / requestEmailApproval are server-mediated (gRPC round-trip like
// sign/encrypt): deliverability and abuse control cannot be enforced from
// inside arbitrary user JS. checkEmailApproval ALSO round-trips, but the
// returned attestation is verified HERE, inside the runtime — if verification
// lived outside the TEE, a compromised approval service could forge approvals
// and move funds (plan D6: in-TEE verification is non-negotiable, so this op
// fails closed when no attestation pubkey is pinned).

/// Pinned secp256k1 pubkey (hex, SEC1 compressed or uncompressed) of the
/// approval service's attestation key. Set on the runtime process inside the
/// enclave at deploy time.
const APPROVAL_ATTESTATION_PUBKEY_ENV: &str = "LIT_APPROVAL_ATTESTATION_PUBKEY";

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim().trim_start_matches("0x");
    if s.len() % 2 != 0 {
        return Err("odd-length hex".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

#[derive(serde::Deserialize)]
struct ApprovalAttestationEnvelope {
    v: String,
    alg: String,
    /// The exact JSON string whose bytes are signed.
    payload: String,
    /// 64-byte compact ECDSA signature, hex.
    sig: String,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct ApprovalAttestationPayload {
    schema: String,
    approval_id: String,
    approver: String,
    assurance: String,
    status: String,
    approved_at_ms: u64,
    expires_at_ms: u64,
}

/// Verify an email-approval-v1 attestation against the pinned network pubkey.
/// Every failure is terminal: a checkEmailApproval that cannot prove approval
/// reports it as an error, never as `approved: true`.
fn verify_approval_attestation(
    approval_id: &str,
    attestation: &str,
) -> Result<ApprovalAttestationPayload, String> {
    use k256::ecdsa::signature::Verifier;

    let pubkey_hex = std::env::var(APPROVAL_ATTESTATION_PUBKEY_ENV).map_err(|_| {
        format!("{APPROVAL_ATTESTATION_PUBKEY_ENV} is not configured on this runtime — refusing to trust an unverifiable approval (fail closed)")
    })?;
    let verifying_key =
        k256::ecdsa::VerifyingKey::from_sec1_bytes(&hex_decode(&pubkey_hex)?).map_err(|e| {
            format!("invalid {APPROVAL_ATTESTATION_PUBKEY_ENV}: {e}")
        })?;

    let envelope: ApprovalAttestationEnvelope =
        serde_json::from_str(attestation).map_err(|e| format!("malformed attestation: {e}"))?;
    if envelope.v != "email-approval-v1" || envelope.alg != "secp256k1-sha256" {
        return Err(format!(
            "unsupported attestation version/alg: {}/{}",
            envelope.v, envelope.alg
        ));
    }
    let signature = k256::ecdsa::Signature::from_slice(&hex_decode(&envelope.sig)?)
        .map_err(|e| format!("malformed attestation signature: {e}"))?;
    verifying_key
        .verify(envelope.payload.as_bytes(), &signature)
        .map_err(|_| "attestation signature verification FAILED".to_string())?;

    let payload: ApprovalAttestationPayload = serde_json::from_str(&envelope.payload)
        .map_err(|e| format!("malformed attestation payload: {e}"))?;
    if payload.schema != "email-approval-v1" {
        return Err(format!("unexpected payload schema: {}", payload.schema));
    }
    if payload.approval_id != approval_id {
        return Err("attestation is for a different approvalId".to_string());
    }
    if payload.status != "approved" {
        return Err(format!("attestation status is '{}', not 'approved'", payload.status));
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(u64::MAX);
    if payload.expires_at_ms <= now_ms {
        return Err("attestation has expired".to_string());
    }
    Ok(payload)
}

#[op2(reentrant)]
async fn op_send_email(
    state: Rc<RefCell<OpState>>,
    #[string] to: String,
    #[string] subject: String,
    #[string] text: String,
) -> Result<(), JsErrorBox> {
    ensure_not_blank!(to, "to");
    ensure_not_blank!(subject, "subject");
    ensure_not_blank!(text, "text");

    remote_op_async!(op_send_email,
        state,
        SendEmailRequest { to, subject, text },
        UnionRequest::SendEmail(_) => Ok(())
    )
}

#[derive(serde::Serialize)]
struct RequestEmailApprovalResult {
    approval_id: String,
    otp: Option<String>,
    approval_url: Option<String>,
}

#[op2(reentrant)]
#[serde]
async fn op_request_email_approval(
    state: Rc<RefCell<OpState>>,
    #[string] to: String,
    #[string] summary: String,
    #[string] assurance: String,
    ttl_sec: u32,
) -> Result<RequestEmailApprovalResult, JsErrorBox> {
    ensure_not_blank!(to, "to");
    ensure_not_blank!(summary, "summary");
    ensure_one_of!(assurance.as_str(), "assurance", ["L1", "L2"]);

    remote_op_async!(op_request_email_approval,
        state,
        RequestEmailApprovalRequest { to, summary, assurance, ttl_sec },
        UnionRequest::RequestEmailApproval(resp) => Ok(RequestEmailApprovalResult {
            approval_id: resp.approval_id,
            otp: Some(resp.otp).filter(|s| !s.is_empty()),
            approval_url: Some(resp.approval_url).filter(|s| !s.is_empty()),
        })
    )
}

#[derive(serde::Serialize)]
struct CheckEmailApprovalResult {
    approved: bool,
    status: String,
    attestation: Option<String>,
    approver: Option<String>,
    assurance: Option<String>,
    approved_at_ms: Option<u64>,
}

#[op2(reentrant)]
#[serde]
async fn op_check_email_approval(
    state: Rc<RefCell<OpState>>,
    #[string] approval_id: String,
) -> Result<CheckEmailApprovalResult, JsErrorBox> {
    ensure_not_blank!(approval_id, "approvalId");

    let (status, attestation) = remote_op_async!(op_check_email_approval,
        state,
        CheckEmailApprovalRequest { approval_id: approval_id.clone() },
        UnionRequest::CheckEmailApproval(resp) => Ok((resp.status, resp.attestation))
    )?;

    if status != "approved" {
        return Ok(CheckEmailApprovalResult {
            approved: false,
            status,
            attestation: None,
            approver: None,
            assurance: None,
            approved_at_ms: None,
        });
    }
    if attestation.is_empty() {
        return Err(JsErrorBox::generic(
            "op_check_email_approval: server reported approved but supplied no attestation (fail closed)",
        ));
    }
    let payload = verify_approval_attestation(&approval_id, &attestation)
        .map_err(|e| JsErrorBox::generic(format!("op_check_email_approval: {e}")))?;
    Ok(CheckEmailApprovalResult {
        approved: true,
        status,
        attestation: Some(attestation),
        approver: Some(payload.approver),
        assurance: Some(payload.assurance),
        approved_at_ms: Some(payload.approved_at_ms),
    })
}

// Build a deno_core::Extension providing custom ops
extension!(
    lit_actions,
    deps = [runtime],
    ops = [
        op_aes_decrypt,
        op_aes_encrypt,
        op_check_email_approval,
        op_get_lit_action_private_key,
        op_get_lit_action_public_key,
        op_get_lit_action_wallet_address,
        op_get_private_key,
        op_increment_fetch_count,
        op_lit_proxied_fetch,
        op_request_email_approval,
        op_send_email,
        op_set_response,
        op_show_import_details,
        op_update_resource_usage,
    ],
    esm_entry_point = "ext:lit_actions/99_patches.js",
    esm = [
        dir "js",
        "00_ethers.js",
        "00_viem.js",
        "02_litActionsSDK.js",
        "99_patches.js",
    ],
    middleware = |op| match op.name {
        "op_print" => op_print(),
        "op_exit" | "op_set_exit_code" => op.with_implementation_from(&op_exit()),
        _ => op,
    },
);
