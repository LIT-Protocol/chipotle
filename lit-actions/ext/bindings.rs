use std::cell::RefCell;
use std::rc::Rc;

use deno_core::{OpState, extension, op2};
use deno_error::JsErrorBox;
use lit_actions_grpc::proto::*;
use tracing::instrument;

use crate::macros::*;

/// Native ECDSA key derivation: computes Ethereum address and uncompressed public key
/// from a hex-encoded private key. ~100x faster than ethers.js Wallet constructor
/// because it uses k256 (Rust native) instead of BN.js (JavaScript).
#[op2]
#[serde]
fn op_derive_eth_address(
    #[string] private_key_hex: &str,
) -> Result<serde_json::Value, JsErrorBox> {
    use k256::ecdsa::SigningKey;
    use k256::elliptic_curve::sec1::ToEncodedPoint;
    use tiny_keccak::{Hasher, Keccak};

    let hex_str = private_key_hex.strip_prefix("0x").unwrap_or(private_key_hex);
    let key_bytes = hex::decode(hex_str)
        .map_err(|e| JsErrorBox::generic(format!("invalid hex private key: {e}")))?;

    let signing_key = SigningKey::from_slice(&key_bytes)
        .map_err(|e| JsErrorBox::generic(format!("invalid private key: {e}")))?;

    let verifying_key = signing_key.verifying_key();
    let public_key_point = verifying_key.to_encoded_point(false);
    let public_key_bytes = public_key_point.as_bytes(); // 65 bytes: 0x04 || x || y

    // Ethereum address = keccak256(public_key_bytes[1..]) last 20 bytes
    let mut hasher = Keccak::v256();
    let mut hash = [0u8; 32];
    hasher.update(&public_key_bytes[1..]); // skip the 0x04 prefix
    hasher.finalize(&mut hash);

    let address = format!("0x{}", hex::encode(&hash[12..]));
    let public_key = format!("0x{}", hex::encode(public_key_bytes));

    Ok(serde_json::json!({
        "address": address,
        "publicKey": public_key,
    }))
}

/// Native ECDSA message signing. Signs an Ethereum personal message using
/// the secp256k1 curve. Returns the signature as a hex string.
#[op2]
#[string]
fn op_sign_message(
    #[string] private_key_hex: &str,
    #[string] message: &str,
) -> Result<String, JsErrorBox> {
    use k256::ecdsa::{SigningKey, signature::hazmat::PrehashSigner};
    use tiny_keccak::{Hasher, Keccak};

    let hex_str = private_key_hex.strip_prefix("0x").unwrap_or(private_key_hex);
    let key_bytes = hex::decode(hex_str)
        .map_err(|e| JsErrorBox::generic(format!("invalid hex private key: {e}")))?;

    let signing_key = SigningKey::from_slice(&key_bytes)
        .map_err(|e| JsErrorBox::generic(format!("invalid private key: {e}")))?;

    // Ethereum personal_sign: hash = keccak256("\x19Ethereum Signed Message:\n" + len + message)
    let prefix = format!("\x19Ethereum Signed Message:\n{}", message.len());
    let mut hasher = Keccak::v256();
    let mut hash = [0u8; 32];
    hasher.update(prefix.as_bytes());
    hasher.update(message.as_bytes());
    hasher.finalize(&mut hash);

    let (signature, recovery_id) = signing_key
        .sign_prehash(&hash)
        .map_err(|e| JsErrorBox::generic(format!("signing failed: {e}")))?;

    // Ethereum signature format: r (32 bytes) + s (32 bytes) + v (1 byte, 27 or 28)
    let mut sig_bytes = [0u8; 65];
    sig_bytes[..64].copy_from_slice(&signature.to_bytes());
    sig_bytes[64] = recovery_id.to_byte() + 27;

    Ok(format!("0x{}", hex::encode(sig_bytes)))
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

    // Store logs locally to avoid gRPC round-trip
    if let Some(local_state) = state.try_borrow_mut::<crate::LocalExecutionState>() {
        local_state.logs.push_str(msg);
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
    // Store response locally to avoid gRPC round-trip
    if let Some(local_state) = state.try_borrow_mut::<crate::LocalExecutionState>() {
        local_state.response = response;
        return Ok(());
    }

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
    // Fast path: use pre-fetched key if available (avoids gRPC round-trip)
    {
        let borrowed = state.borrow();
        if let Some(key) = borrowed.try_borrow::<crate::PrefetchedLitActionKey>() {
            return Ok(key.0.clone());
        }
    }
    // Fallback: request key via gRPC
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
        op_derive_eth_address,
        op_sign_message,
        op_get_lit_action_private_key,
        op_get_lit_action_public_key,
        op_get_lit_action_wallet_address,
        op_get_private_key,
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
