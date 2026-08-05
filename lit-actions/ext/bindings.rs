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
    /// `http(s)://[user:pass@]host:port`. Omit/`null` = deliberate direct
    /// (unproxied) request. A supplied-but-empty/whitespace value is rejected
    /// (fail closed) rather than degrading to direct egress.
    #[serde(default)]
    proxy: Option<String>,
}

fn default_proxied_fetch_method() -> String {
    "GET".to_string()
}

/// Split `scheme://user:pass@host:port` into (`scheme://host:port`, Some((user, pass))).
/// Userinfo is taken up to the last `@` before the host; the user/pass split is
/// on the first `:`. Returns no credentials when there is no userinfo.
///
/// User and pass are percent-decoded: per RFC 3986 the userinfo component is
/// percent-encoded, and proxy providers (Webshare, Bright Data, ...) hand out
/// generated passwords that contain reserved chars (`@ : / +`) which therefore
/// arrive `%`-encoded. We must decode to the original bytes before handing them
/// to `.basic_auth()`, or authentication fails against the proxy. The decode is
/// `_lossy` so a malformed `%XX` degrades to U+FFFD rather than dropping the
/// credential entirely.
fn split_proxy_credentials(proxy_url: &str) -> (String, Option<(String, String)>) {
    use percent_encoding::percent_decode_str;

    let Some((scheme, rest)) = proxy_url.split_once("://") else {
        return (proxy_url.to_string(), None);
    };
    let Some((userinfo, hostport)) = rest.rsplit_once('@') else {
        return (proxy_url.to_string(), None);
    };
    let decode = |s: &str| percent_decode_str(s).decode_utf8_lossy().into_owned();
    let (user, pass) = match userinfo.split_once(':') {
        Some((u, p)) => (decode(u), decode(p)),
        None => (decode(userinfo), String::new()),
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

/// Fallback permit count when no `ProxiedFetchLimiter` was installed in
/// `OpState` (i.e. the op is driven outside the server's execution path, which
/// always installs one sized to the isolate's memory budget). Mirrors the
/// server's 64 MiB default budget / 10 MiB per fetch = 6, so a stray direct
/// invocation is still bounded rather than running unbounded.
const PROXIED_FETCH_FALLBACK_PERMITS: usize = 6;

/// Per-execution cap on how many `op_lit_proxied_fetch` calls may be buffering a
/// response body natively at the same time.
///
/// Each in-flight call holds up to `PROXIED_FETCH_MAX_BYTES` (10 MiB) in a native
/// `Vec` that lives OUTSIDE the V8 heap, so it is invisible to the isolate's
/// `add_near_heap_limit_callback` OOM guard. Without a concurrency cap,
/// `Promise.all(N × proxiedFetch(bigUrl))` grows RSS to N × 10 MiB with nothing
/// to stop it short of the host OOM killer (CPL-373). The permit is held across
/// the request + streamed read and released on return, so excess concurrent
/// fetches wait for a permit here instead of each pinning up to 10 MiB of
/// off-heap RSS. Cloneable: the inner `Arc<Semaphore>` is shared, so every clone
/// draws from the same permit pool.
#[derive(Clone)]
pub struct ProxiedFetchLimiter(Arc<tokio::sync::Semaphore>);

impl ProxiedFetchLimiter {
    /// Size the permit count so `permits × PROXIED_FETCH_MAX_BYTES` tracks the
    /// isolate's heap budget: combined native buffering stays on the order of
    /// the memory the isolate is already allowed and no more. Always at least
    /// one permit so a lone fetch is never blocked, even for a tiny custom
    /// `memory_limit_mb`.
    pub fn for_memory_budget_mb(memory_limit_mb: usize) -> Self {
        let budget_bytes = memory_limit_mb.saturating_mul(1024 * 1024);
        let permits = (budget_bytes / PROXIED_FETCH_MAX_BYTES).max(1);
        Self(Arc::new(tokio::sync::Semaphore::new(permits)))
    }

    /// Acquire one buffering slot, awaiting a free permit if all are in use.
    /// The returned permit must be held for the lifetime of the native buffer;
    /// dropping it frees the slot for a waiting fetch.
    async fn acquire(&self) -> Result<tokio::sync::OwnedSemaphorePermit, JsErrorBox> {
        // `acquire_owned` only errors if the semaphore is closed, which we never
        // do — the limiter lives as long as the execution's OpState.
        Arc::clone(&self.0)
            .acquire_owned()
            .await
            .map_err(|_| JsErrorBox::generic("op_lit_proxied_fetch: fetch limiter unavailable"))
    }
}

impl Default for ProxiedFetchLimiter {
    fn default() -> Self {
        Self(Arc::new(tokio::sync::Semaphore::new(
            PROXIED_FETCH_FALLBACK_PERMITS,
        )))
    }
}

lazy_static::lazy_static! {
    /// One `reqwest::Client` per proxy URL ("" = direct). `Client` is an Arc
    /// around a connection pool, so reuse keeps TCP+TLS sessions warm across
    /// calls instead of paying a fresh handshake per request (plan M2
    /// follow-up). `RwLock` (not `Mutex`): the pool saturates at <=16 entries
    /// almost immediately and is read-only thereafter, so concurrent lookups
    /// proceed in parallel. Keys contain proxy credentials — they are secret
    /// material and must never be logged.
    static ref PROXIED_FETCH_CLIENTS: std::sync::RwLock<std::collections::HashMap<String, reqwest::Client>> =
        std::sync::RwLock::new(std::collections::HashMap::new());
}

/// Resolve the request's `proxy` field into the value handed to the client pool.
///
/// Fail closed on a supplied-but-empty proxy. `None` (field absent / `null`) is a
/// deliberate direct request. `Some(non-empty)` is proxied (trimmed). But a proxy
/// that is present yet blank/whitespace is a misconfiguration (empty env var, bad
/// string interpolation in an action), NOT a request for direct egress — silently
/// degrading it to a direct request would send the signed request + API keys +
/// HMAC out the enclave's own (geo-blocked) IP, precisely what this op exists to
/// prevent. So that case is an error, not a fallback.
fn resolve_proxy(proxy: Option<&str>) -> Result<Option<&str>, JsErrorBox> {
    match proxy {
        Some(p) if p.trim().is_empty() => Err(JsErrorBox::generic(
            "op_lit_proxied_fetch: proxy was supplied but is empty/whitespace; \
             omit the field (or pass null) for a deliberate direct request",
        )),
        Some(p) => Ok(Some(p.trim())),
        None => Ok(None),
    }
}

fn proxied_fetch_client(proxy: Option<&str>) -> Result<reqwest::Client, JsErrorBox> {
    let key = proxy.unwrap_or_default();
    if let Some(client) = PROXIED_FETCH_CLIENTS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(key)
    {
        return Ok(client.clone());
    }

    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(PROXIED_FETCH_TIMEOUT_SECS))
        // No automatic redirect following (parity with the CDN module loader). A
        // venue/proxy/attacker-controlled endpoint could otherwise 30x-redirect a
        // signed request to a different host, an internal address, or an http://
        // downgrade — replaying the caller's method, body, and API-key/HMAC headers
        // to a destination the action never authorized. The action sees the 3xx
        // status and decides for itself.
        .redirect(reqwest::redirect::Policy::none());
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
        .write()
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
/// `fetch` egresses from. For an `https://` destination the proxy is reached
/// over an HTTP CONNECT tunnel, so TLS to the venue is end-to-end and the proxy
/// sees only the host:port, never the request URL, headers, or body. NOTE: this
/// end-to-end guarantee holds ONLY for `https://` targets. A plain `http://`
/// destination is sent as an ordinary forward-proxy request, so the proxy sees
/// the full URL/headers/body in cleartext — callers handling secrets must use
/// `https://`. The per-action fetch quota is enforced by the JS wrapper calling
/// `op_increment_fetch_count` before this op; the byte cap bounds a single
/// response the way `deno_fetch` does, and a per-execution `ProxiedFetchLimiter`
/// caps how many responses buffer natively at once so concurrent calls can't
/// amplify off-heap RSS past the isolate's memory budget (CPL-373). Unlike the
/// other ops here it does no gRPC round-trip to lit-node — the request is purely
/// local.
//
// NB: `async(lazy)`, not bare `async`: in edition 2024 a bare `async` keyword in
// attribute position fails to parse ("expected `async(...)`"). The other async
// ops here sidestep it via the `reentrant` *identifier*; we want plain async
// semantics, and the parenthesized `async(lazy)` form parses cleanly and is a
// real (non-fake) async op. No `#[instrument]` (it re-mangles the keyword too).
#[op2(async(lazy))]
#[serde]
async fn op_lit_proxied_fetch(
    state: Rc<RefCell<OpState>>,
    #[serde] req: ProxiedFetchRequest,
) -> Result<ProxiedFetchResponse, JsErrorBox> {
    ensure_not_blank!(req.url, "url");

    // Bound concurrent native response buffering per execution (CPL-373). The
    // server installs a `ProxiedFetchLimiter` sized to the isolate's memory
    // budget; if one is somehow absent (a direct op call outside that path), we
    // install a conservative default so all calls in this execution still share
    // one permit pool rather than each running unbounded.
    let limiter = {
        let mut state = state.borrow_mut();
        if let Some(limiter) = state.try_borrow::<ProxiedFetchLimiter>() {
            limiter.clone()
        } else {
            let limiter = ProxiedFetchLimiter::default();
            state.put(limiter.clone());
            limiter
        }
    };
    // Held until this op returns, gating both the in-flight request and the
    // native read buffer below. Released on drop so a waiting fetch can proceed.
    let _permit = limiter.acquire().await?;

    let client = proxied_fetch_client(resolve_proxy(req.proxy.as_deref())?)?;

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
        // Deliberately categorized only — do NOT interpolate `{e}` or its
        // source chain. reqwest embeds the request URL in its Display, and for
        // signed venue calls (e.g. Binance) the HMAC signature + order params
        // ride in the query string; leaking them into an action error/log
        // returned to the caller would disclose request-signing material.
        JsErrorBox::generic(format!(
            "op_lit_proxied_fetch: request failed (timeout={} connect={} request={} body={})",
            e.is_timeout(),
            e.is_connect(),
            e.is_request(),
            e.is_body(),
        ))
    })?;

    let status = resp.status().as_u16();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();

    // Streamed read with a hard cap so a hostile or runaway response can't blow
    // the action's 64MB memory budget.
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| {
        // Categorized only — same reasoning as the send() error above. A read
        // error after headers (timeout/reset/decode) still carries the request
        // URL in reqwest's Display, which for signed venue calls embeds the HMAC
        // signature + order params in the query string. Never interpolate `{e}`.
        JsErrorBox::generic(format!(
            "op_lit_proxied_fetch: body read failed (timeout={} body={} decode={})",
            e.is_timeout(),
            e.is_body(),
            e.is_decode(),
        ))
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
        // TEXT-ONLY contract: the body crosses into JS as a string, so a
        // non-UTF-8 (binary/gzipped/protobuf) response is replaced lossily with
        // U+FFFD rather than round-tripping. Every current consumer is a venue
        // REST API returning JSON/UTF-8 text, for which this is exact. Binary
        // responses are out of scope for v1; supporting them (base64 envelope or
        // a responseType flag) is a follow-up. Documented on the JS wrapper.
        body: String::from_utf8_lossy(&buf).into_owned(),
    })
}

// Build a deno_core::Extension providing custom ops
extension!(
    lit_actions,
    deps = [runtime],
    ops = [
        op_aes_decrypt,
        op_aes_encrypt,
        op_get_lit_action_private_key,
        op_get_lit_action_public_key,
        op_get_lit_action_wallet_address,
        op_get_private_key,
        op_increment_fetch_count,
        op_lit_proxied_fetch,
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

#[cfg(test)]
mod proxied_fetch_tests {
    use super::*;

    #[test]
    fn no_scheme_returns_input_unchanged() {
        // Not a URL with a scheme — pass through, no credentials.
        let (url, creds) = split_proxy_credentials("not-a-url");
        assert_eq!(url, "not-a-url");
        assert_eq!(creds, None);
    }

    #[test]
    fn no_userinfo_returns_input_unchanged() {
        // A direct proxy with no `user:pass@` must keep credentials None so the
        // builder never calls `.basic_auth()` with empty strings.
        let (url, creds) = split_proxy_credentials("http://proxy.example:8080");
        assert_eq!(url, "http://proxy.example:8080");
        assert_eq!(creds, None);
    }

    #[test]
    fn user_and_pass_are_split_and_stripped_from_url() {
        let (url, creds) = split_proxy_credentials("http://alice:s3cret@proxy.example:8080");
        assert_eq!(url, "http://proxy.example:8080");
        assert_eq!(creds, Some(("alice".to_string(), "s3cret".to_string())));
    }

    #[test]
    fn user_without_colon_gets_empty_password() {
        let (url, creds) = split_proxy_credentials("https://tokenonly@proxy.example:443");
        assert_eq!(url, "https://proxy.example:443");
        assert_eq!(creds, Some(("tokenonly".to_string(), String::new())));
    }

    #[test]
    fn percent_encoded_reserved_chars_are_decoded() {
        // Generated proxy passwords routinely contain reserved chars delivered
        // %-encoded: `:` (%3A), `/` (%2F), `+` (%2B), `@` (%40). The decoded
        // bytes are what must reach `.basic_auth()`.
        let (url, creds) =
            split_proxy_credentials("http://user%40corp:p%40ss%2Fw%2Brd%3A1@proxy.example:8080");
        assert_eq!(url, "http://proxy.example:8080");
        assert_eq!(
            creds,
            Some(("user@corp".to_string(), "p@ss/w+rd:1".to_string()))
        );
    }

    #[test]
    fn password_containing_at_sign_splits_on_last_at() {
        // userinfo is taken up to the LAST `@`, so a literal `@` mid-password
        // (when not percent-encoded) still leaves the real host intact.
        let (url, creds) = split_proxy_credentials("http://u:p@ss@proxy.example:8080");
        assert_eq!(url, "http://proxy.example:8080");
        assert_eq!(creds, Some(("u".to_string(), "p@ss".to_string())));
    }

    #[test]
    fn default_method_is_get() {
        assert_eq!(default_proxied_fetch_method(), "GET");
    }

    #[test]
    fn resolve_proxy_none_is_direct() {
        // Field absent / null = deliberate direct request.
        assert_eq!(resolve_proxy(None).unwrap(), None);
    }

    #[test]
    fn resolve_proxy_present_is_trimmed() {
        assert_eq!(
            resolve_proxy(Some("  http://proxy.example:8080  ")).unwrap(),
            Some("http://proxy.example:8080")
        );
    }

    #[test]
    fn resolve_proxy_supplied_but_empty_fails_closed() {
        // The security-critical case: a present-but-blank proxy must NOT degrade
        // to direct egress — it's an error.
        assert!(resolve_proxy(Some("")).is_err());
        assert!(resolve_proxy(Some("   ")).is_err());
        assert!(resolve_proxy(Some("\t\n")).is_err());
    }

    #[test]
    fn byte_cap_is_ten_mib() {
        // Guards the memory bound the op relies on; a silent bump here would let
        // a single response grow the native buffer past the documented limit.
        assert_eq!(PROXIED_FETCH_MAX_BYTES, 10 * 1024 * 1024);
    }

    #[test]
    fn limiter_permits_track_memory_budget() {
        // permits = budget / 10 MiB, so combined native buffering
        // (permits × 10 MiB) stays on the order of the isolate's heap budget.
        assert_eq!(
            ProxiedFetchLimiter::for_memory_budget_mb(64)
                .0
                .available_permits(),
            6 // 64 MiB / 10 MiB
        );
        assert_eq!(
            ProxiedFetchLimiter::for_memory_budget_mb(128)
                .0
                .available_permits(),
            12 // 128 MiB / 10 MiB
        );
    }

    #[test]
    fn limiter_floors_at_one_permit_for_tiny_budgets() {
        // A budget below a single fetch's cap must still allow one fetch — a
        // 0-permit semaphore would deadlock every proxied fetch forever.
        assert_eq!(
            ProxiedFetchLimiter::for_memory_budget_mb(8)
                .0
                .available_permits(),
            1
        );
        assert_eq!(
            ProxiedFetchLimiter::for_memory_budget_mb(0)
                .0
                .available_permits(),
            1
        );
    }

    #[test]
    fn limiter_default_matches_fallback_permits() {
        assert_eq!(
            ProxiedFetchLimiter::default().0.available_permits(),
            PROXIED_FETCH_FALLBACK_PERMITS
        );
    }

    #[test]
    fn limiter_blocks_once_permits_exhausted() {
        // The concurrency bound that stops native-memory amplification
        // (CPL-373): with one slot, a second concurrent fetch finds no permit
        // and must wait until the first releases.
        let limiter = ProxiedFetchLimiter::for_memory_budget_mb(8); // 1 permit
        let held = Arc::clone(&limiter.0)
            .try_acquire_owned()
            .expect("first slot free");
        assert!(
            limiter.0.try_acquire().is_err(),
            "second concurrent fetch must wait for a permit"
        );
        drop(held);
        assert!(
            limiter.0.try_acquire().is_ok(),
            "permit is freed once the first fetch's buffer is released"
        );
    }
}
