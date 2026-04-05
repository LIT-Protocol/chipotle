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
    use std::io::{Write, stderr, stdout};

    lazy_static::lazy_static! {
        static ref IS_ATTY_STDOUT: bool = atty::is(atty::Stream::Stdout);
        static ref IS_ATTY_STDERR: bool = atty::is(atty::Stream::Stderr);
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
#[op2(async, reentrant)]
async fn op_increment_fetch_count(state: Rc<RefCell<OpState>>) -> Result<u32, JsErrorBox> {
    remote_op_async!(op_increment_fetch_count,
        state,
        IncrementFetchCountRequest {},
        UnionRequest::IncrementFetchCount(resp) => Ok(resp.fetch_count)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
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
#[op2(async, reentrant)]
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
#[op2(async, reentrant)]
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
#[op2(async, reentrant)]
#[string]
async fn op_get_lit_action_private_key(state: Rc<RefCell<OpState>>) -> Result<String, JsErrorBox> {
    remote_op_async!(op_get_lit_action_private_key,
        state,
        GetLitActionPrivateKeyRequest {},
        UnionRequest::GetLitActionPrivateKey(resp) => Ok(resp.secret)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
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
#[op2(async, reentrant)]
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
#[op2(async, reentrant)]
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
