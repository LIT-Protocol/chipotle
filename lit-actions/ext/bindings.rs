use std::cell::RefCell;
use std::rc::Rc;

use deno_core::{OpState, extension, op2};
use deno_error::JsErrorBox;
use lit_actions_grpc::proto::*;
use serde_json::json;
use tracing::instrument;

use crate::macros::*;

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
async fn op_sign(
    state: Rc<RefCell<OpState>>,
    #[buffer(copy)] to_sign: Vec<u8>,
    #[string] public_key: String,
    #[string] sig_name: String,
    #[string] signing_scheme: String,
    #[string] _key_set_id: String,
) -> Result<String, JsErrorBox> {
    ensure_not_empty!(to_sign, "toSign");
    ensure_not_blank!(public_key, "publicKey");
    ensure_not_blank!(sig_name, "sigName");
    ensure_not_blank!(signing_scheme, "signingScheme");

    remote_op_async!(op_sign,
        state,
        SignRequest {
            to_sign,
            public_key,
            sig_name,
            signing_scheme,
        },
        UnionRequest::Sign(resp) => Ok(resp.success)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
#[string]
async fn op_sign_as_action(
    state: Rc<RefCell<OpState>>,
    #[buffer(copy)] to_sign: Vec<u8>,
    #[string] sig_name: String,
    #[string] signing_scheme: String,
) -> Result<String, JsErrorBox> {
    ensure_not_empty!(to_sign, "toSign");
    ensure_not_blank!(sig_name, "sigName");
    ensure_not_blank!(signing_scheme, "signingScheme");

    remote_op_async!(op_sign_as_action,
        state,
        SignAsActionRequest {
            to_sign,
            sig_name,
            signing_scheme,
        },
        UnionRequest::SignAsAction(resp) => Ok(resp.result)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
#[string]
async fn op_get_action_public_key(
    state: Rc<RefCell<OpState>>,
    #[string] signing_scheme: String,
    #[string] action_ipfs_cid: String,
) -> Result<String, JsErrorBox> {
    ensure_not_blank!(signing_scheme, "signingScheme");
    ensure_not_blank!(action_ipfs_cid, "actionIpfsCid");

    remote_op_async!(op_get_action_public_key,
        state,
        GetActionPublicKeyRequest {
            signing_scheme,
            action_ipfs_cid,
        },
        UnionRequest::GetActionPublicKey(resp) => Ok(resp.result)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
async fn op_verify_action_signature(
    state: Rc<RefCell<OpState>>,
    #[string] signing_scheme: String,
    #[string] action_ipfs_cid: String,
    #[buffer(copy)] to_sign: Vec<u8>,
    #[string] sign_output: String,
) -> Result<bool, JsErrorBox> {
    ensure_not_empty!(to_sign, "toSign");
    ensure_not_blank!(signing_scheme, "signingScheme");
    ensure_not_blank!(action_ipfs_cid, "actionIpfsCid");
    ensure_not_blank!(sign_output, "signOutput");

    remote_op_async!(op_verify_action_signature,
        state,
        VerifyActionSignatureRequest {
            signing_scheme,
            action_ipfs_cid,
            to_sign,
            sign_output,
        },
        UnionRequest::VerifyActionSignature(resp) => Ok(resp.result)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
#[string]
async fn op_aes_encrypt(
    state: Rc<RefCell<OpState>>,
    #[string] public_key: String,
    #[string] message: String,
) -> Result<String, JsErrorBox> {
    ensure_not_blank!(public_key, "publicKey");
    ensure_not_blank!(message, "message");

    remote_op_async!(op_aes_encrypt,
        state,
        AesEncryptRequest { public_key, message },
        UnionRequest::AesEncrypt(resp) => Ok(resp.ciphertext)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
#[string]
async fn op_aes_decrypt(
    state: Rc<RefCell<OpState>>,
    #[string] public_key: String,
    #[string] ciphertext: String,
) -> Result<String, JsErrorBox> {
    ensure_not_empty!(ciphertext);

    remote_op_async!(op_aes_decrypt,
        state,
        AesDecryptRequest { public_key, ciphertext },
        UnionRequest::AesDecrypt(resp) => Ok(resp.plaintext)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
#[string]
async fn op_get_latest_nonce(
    state: Rc<RefCell<OpState>>,
    #[serde] address: ethabi::ethereum_types::Address,
    #[string] chain: String,
) -> Result<String, JsErrorBox> {
    ensure_not_blank!(chain);

    remote_op_async!(op_get_latest_nonce,
        state,
        GetLatestNonceRequest {
            address: format!("{address:x}"),
            chain,
        },
        UnionRequest::GetLatestNonce(resp) => Ok(resp.nonce)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
#[string]
async fn op_call_contract(
    state: Rc<RefCell<OpState>>,
    #[string] chain: String,
    #[string] txn: String,
) -> Result<String, JsErrorBox> {
    ensure_not_blank!(chain); // ensure_one_of not feasible as there are too many supported blockchains
    ensure_not_blank!(txn);

    remote_op_async!(op_call_contract,
        state,
        CallContractRequest { chain, txn },
        UnionRequest::CallContract(resp) => Ok(resp.result)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
#[string]
async fn op_call_child(
    state: Rc<RefCell<OpState>>,
    #[string] ipfs_id: String,
    #[serde] params: Option<serde_json::Value>,
) -> Result<String, JsErrorBox> {
    ensure_not_blank!(ipfs_id, "ipfsId");

    remote_op_async!(op_call_child,
        state,
        CallChildRequest {
            ipfs_id,
            params: params.as_ref().map(serde_json::to_vec).transpose().map_err(JsErrorBox::from_err)?,
        },
        UnionRequest::CallChild(resp) => Ok(resp.response)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
#[string]
async fn op_get_rpc_url(
    state: Rc<RefCell<OpState>>,
    #[string] chain: String,
) -> Result<String, JsErrorBox> {
    remote_op_async!(op_get_rpc_url,
        state,
        GetRpcUrlRequest { chain },
        UnionRequest::GetRpcUrl(resp) => Ok(resp.result)
    )
}

#[instrument(skip_all, ret)]
#[op2(async, reentrant)]
#[serde]
async fn op_encrypt_bls(
    state: Rc<RefCell<OpState>>,
    #[serde] access_control_conditions: Vec<serde_json::Value>, // Vec<UnifiedAccessControlConditionItem>
    #[buffer(copy)] to_encrypt: Vec<u8>,
    #[string] _key_set_id: String,
) -> Result<serde_json::Value, JsErrorBox> {
    remote_op_async!(op_encrypt_bls,
        state,
        EncryptBlsRequest {
            access_control_conditions: serde_json::to_vec(&access_control_conditions).map_err(JsErrorBox::from_err)?,
            to_encrypt,
        },
        UnionRequest::EncryptBls(resp) => Ok(json!({"ciphertext": resp.ciphertext, "dataToEncryptHash": resp.data_to_encrypt_hash}))
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
        op_call_child,
        op_call_contract,
        op_get_latest_nonce,
        op_increment_fetch_count,
        op_set_response,
        op_sign,
        op_sign_as_action,
        op_get_action_public_key,
        op_verify_action_signature,
        op_get_rpc_url,
        op_encrypt_bls,
        op_update_resource_usage,
    ],
    esm_entry_point = "ext:lit_actions/99_patches.js",
    esm = [
        dir "js",
        "00_ethers.js",
        "01_uint8arrays.js",
        "02_litActionsSDK.js",
        "03_jsonwebtoken.js",
        "99_patches.js",
    ],
    middleware = |op| match op.name {
        "op_print" => op_print(),
        "op_exit" | "op_set_exit_code" => op.with_implementation_from(&op_exit()),
        _ => op,
    },
);
