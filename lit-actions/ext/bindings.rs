use std::cell::RefCell;
use std::rc::Rc;

use deno_core::{OpState, extension, op2};
use deno_error::JsErrorBox;
use lit_actions_grpc::proto::*;
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
        op_increment_fetch_count,
        op_set_response,
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
