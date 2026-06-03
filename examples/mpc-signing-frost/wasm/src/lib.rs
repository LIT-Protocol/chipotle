//! wasm-bindgen wrapper over Lit's audited FROST libraries, fixed to the
//! **Ed25519** ciphersuite (Solana's native signature scheme):
//!
//!   * `frost-dkg`  — real distributed key generation (no trusted dealer; the
//!                    full key never exists in one place).
//!   * `lit-frost`  — the 2-round FROST signing protocol + aggregation.
//!
//! The SAME compiled wasm runs in two places (the `mpc-signing-frost` example
//! mirrors `mpc-signing-ecdsa` here): the **web** build inside the Lit Action
//! (Deno), and the **node** build on the user's machine. The wrapper deals only
//! in byte blobs so the caller can seal/relay them — exactly the transport model
//! the Lit Action needs (it is stateless across HTTP calls, so its per-round
//! state is serialized out, sealed to the PKP, and replayed back next round).
//!
//! Byte conventions:
//!   * lit-frost values (`SigningShare`, `VerifyingKey`, `VerifyingShare`,
//!     `SigningNonces`, `SigningCommitments`, `SignatureShare`) cross the
//!     boundary as their `serde_bare` bytes — `Vec::<u8>::from(&x)` out,
//!     `T::try_from(&[u8])` in.
//!   * a finished signature crosses as the raw **64-byte** Ed25519 signature
//!     (`Signature.value`) — submit-ready for Solana.
//!   * the group public key also crosses as raw **32 bytes** (`solana_pubkey`),
//!     which IS the Solana address (base58-encode it on the JS side).
//!
//! DKG relay model: there are 3 FROST participants (party 0 = user hot, party 1
//! = the Lit Action, party 2 = user cold), identified by the small integers in
//! `all_ids` (e.g. [1,2,3]). The id list and its order MUST be identical in
//! every process — frost-dkg assigns ordinals by id position and routes by
//! ordinal. Each `dkg_*` call returns the messages this participant must send
//! (`{ dst, data }`, `dst` = the recipient's id) and an opaque `state` blob to
//! carry into the next round.

use frost_dkg::*;
use frost_dkg::vsss_rs::{
    curve25519::{WrappedEdwards, WrappedScalar},
    elliptic_curve::{group::GroupEncoding, PrimeField},
    IdentifierPrimeField, ParticipantIdGeneratorType,
};
use lit_frost::{
    Identifier, KeyPackage, Scheme, Signature, SignatureShare, SigningCommitments, SigningNonces,
    SigningShare, VerifyingKey, VerifyingShare,
};
use serde::{Deserialize, Serialize};
use std::num::{NonZeroU16, NonZeroUsize};
use wasm_bindgen::prelude::*;

/// The ciphersuite. Ed25519 = Solana. (lit-frost covers the rest of the FROST
/// family too — flip this and the example signs for Bitcoin Taproot, P-256,
/// Schnorrkel/Substrate, etc.)
const SCHEME: Scheme = Scheme::Ed25519Sha512;

type G = WrappedEdwards;
type Part = SecretParticipant<G>;

fn err(e: impl ToString) -> JsValue {
    JsValue::from_str(&e.to_string())
}

fn to_js<T: Serialize>(v: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(v).map_err(err)
}

fn from_js<T: for<'de> Deserialize<'de>>(v: JsValue) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(v).map_err(err)
}

/// participant id (small integer) -> frost-dkg scalar identifier.
fn id_scalar(n: u16) -> IdentifierPrimeField<WrappedScalar> {
    IdentifierPrimeField(WrappedScalar::from(n as u64))
}

/// lit-frost identifier for the same participant — these MUST encode the same
/// scalar as `id_scalar` (integer n -> the field element n) so signing-round
/// Lagrange interpolation lines up with the DKG shares. They do: both encode n.
fn lit_id(n: u16) -> Identifier {
    Identifier::from((SCHEME, n))
}

// ---------------------------------------------------------------------------
// Wire / boundary types
// ---------------------------------------------------------------------------

/// One outgoing DKG message: relay `data` verbatim to the participant `dst`.
#[derive(Serialize)]
struct Wire {
    dst: u16,
    data: Vec<u8>,
}

/// One incoming DKG message (the `data` another participant relayed to us).
#[derive(Deserialize)]
struct InMsg {
    from: u16,
    data: Vec<u8>,
}

/// Opaque DKG state carried between rounds. `participant` needs serde (see
/// ../README.md "Prerequisite #1").
#[derive(Serialize, Deserialize)]
struct DkgState {
    my_id: u16,
    all_ids: Vec<u16>,
    participant: Part,
}

/// Reject relayed message sets the protocol shouldn't accept before feeding them
/// to frost-dkg: an unknown sender, our own id echoed back, or a duplicate sender
/// (which could double-count toward the round threshold). frost-dkg also
/// authenticates messages internally; this is wrapper-level defense in depth.
fn check_senders(st: &DkgState, msgs: &[InMsg]) -> Result<(), JsValue> {
    let mut seen = std::collections::BTreeSet::new();
    for m in msgs {
        if m.from == st.my_id {
            return Err(err("DKG message claims to be from this party"));
        }
        if !st.all_ids.contains(&m.from) {
            return Err(err(format!("DKG message from unknown party {}", m.from)));
        }
        if !seen.insert(m.from) {
            return Err(err(format!("duplicate DKG message from party {}", m.from)));
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct DkgRoundOut {
    /// carry into the next dkg_* call (the action seals this)
    state: Vec<u8>,
    /// messages to send this round
    out: Vec<Wire>,
}

#[derive(Serialize)]
struct DkgFinal {
    /// this party's lit-frost SigningShare (serde_bare) — the long-lived secret
    signing_share: Vec<u8>,
    /// the group's lit-frost VerifyingKey (serde_bare) — used by sign/aggregate
    verifying_key: Vec<u8>,
    /// this party's lit-frost VerifyingShare (serde_bare)
    verifying_share: Vec<u8>,
    /// raw 32-byte Ed25519 group public key = the Solana address
    solana_pubkey: Vec<u8>,
}

/// A per-signer item keyed by participant id (for signing/aggregation).
#[derive(Serialize, Deserialize)]
struct IdItem {
    id: u16,
    data: Vec<u8>,
}

#[derive(Serialize)]
struct SignRound1Out {
    /// SigningNonces (serde_bare) — SECRET, single-use; the action seals this
    nonce: Vec<u8>,
    /// SigningCommitments (serde_bare) — broadcast to the other signer
    commitment: Vec<u8>,
    /// this signer's VerifyingShare (serde_bare) — the coordinator needs it to aggregate
    verifying_share: Vec<u8>,
}

#[derive(Serialize)]
struct SignRound2Out {
    /// this signer's SignatureShare (serde_bare)
    signature_share: Vec<u8>,
    verifying_share: Vec<u8>,
}

// ---------------------------------------------------------------------------
// DKG — 3 frost-dkg rounds. The user runs its two parties locally; the Lit
// Action runs party 1 across (at most) two HTTP calls: dkg_round1 in the first,
// then dkg_round2 + dkg_round3 back-to-back in the second (round 3 has no
// outgoing messages and only needs round-2 inputs, so it does not need its own
// HTTP round-trip). State is sealed only across the round1 -> round2 gap.
// ---------------------------------------------------------------------------

fn build_params<'a>(
    ids: &'a [IdentifierPrimeField<WrappedScalar>],
    threshold: u16,
) -> Result<Parameters<'a, G>, JsValue> {
    let t = NonZeroUsize::new(threshold as usize).ok_or_else(|| err("threshold must be >= 1"))?;
    let n = NonZeroUsize::new(ids.len()).ok_or_else(|| err("need at least one participant"))?;
    let seq = vec![ParticipantIdGeneratorType::list(ids)];
    Ok(Parameters::<G>::new(t, n, None, Some(seq)))
}

/// Run round 1 for participant `my_id`. Returns its broadcast + carried state.
#[wasm_bindgen]
pub fn dkg_round1(my_id: u16, all_ids: Vec<u16>, threshold: u16) -> Result<JsValue, JsValue> {
    let ids: Vec<_> = all_ids.iter().map(|n| id_scalar(*n)).collect();
    let params = build_params(&ids, threshold)?;
    let mut participant = Part::new_secret(id_scalar(my_id), &params).map_err(err)?;
    let generator = participant.run().map_err(err)?;
    let out = collect_out(&all_ids, &generator);
    finish_round(my_id, all_ids, participant, out)
}

/// Receive the relayed round-1 messages, then run round 2.
#[wasm_bindgen]
pub fn dkg_round2(state: Vec<u8>, incoming: JsValue) -> Result<JsValue, JsValue> {
    let mut st: DkgState = serde_json::from_slice(&state).map_err(err)?;
    let msgs: Vec<InMsg> = from_js(incoming)?;
    check_senders(&st, &msgs)?;
    for m in &msgs {
        st.participant.receive(&m.data).map_err(err)?;
    }
    let generator = st.participant.run().map_err(err)?;
    let out = collect_out(&st.all_ids, &generator);
    finish_round(st.my_id, st.all_ids, st.participant, out)
}

/// Receive the relayed round-2 messages, run round 3 (finalize) and return the
/// long-lived key material. Round 3 emits no messages.
#[wasm_bindgen]
pub fn dkg_round3(state: Vec<u8>, incoming: JsValue) -> Result<JsValue, JsValue> {
    let mut st: DkgState = serde_json::from_slice(&state).map_err(err)?;
    let msgs: Vec<InMsg> = from_js(incoming)?;
    check_senders(&st, &msgs)?;
    for m in &msgs {
        st.participant.receive(&m.data).map_err(err)?;
    }
    let _ = st.participant.run().map_err(err)?; // finalizes; no outgoing messages

    let share = st
        .participant
        .get_secret_share()
        .ok_or_else(|| err("DKG not complete: no secret share"))?;
    let pk = st
        .participant
        .get_public_key()
        .ok_or_else(|| err("DKG not complete: no public key"))?;

    // frost-dkg scalar/point -> lit-frost values (both 32-byte LE/compressed,
    // matching lit-peer's CompressedBytes bridge for Ed25519).
    let share_bytes = share.value.0.to_repr();
    let pk_bytes = pk.to_bytes();

    let signing_share = SigningShare {
        scheme: SCHEME,
        value: share_bytes.as_ref().to_vec(),
    };
    let verifying_key = VerifyingKey {
        scheme: SCHEME,
        value: pk_bytes.as_ref().to_vec(),
    };
    let verifying_share = SCHEME.verifying_share(&signing_share).map_err(err)?;

    to_js(&DkgFinal {
        signing_share: (&signing_share).into(),
        verifying_key: (&verifying_key).into(),
        verifying_share: (&verifying_share).into(),
        solana_pubkey: pk_bytes.as_ref().to_vec(),
    })
}

fn collect_out(all_ids: &[u16], generator: &RoundOutputGenerator<G>) -> Vec<Wire> {
    generator
        .iter()
        .map(|o| Wire {
            dst: all_ids[o.dst_ordinal],
            data: o.data,
        })
        .collect()
}

fn finish_round(my_id: u16, all_ids: Vec<u16>, participant: Part, out: Vec<Wire>) -> Result<JsValue, JsValue> {
    let st = DkgState { my_id, all_ids, participant };
    let state = serde_json::to_vec(&st).map_err(err)?;
    to_js(&DkgRoundOut { state, out })
}

// ---------------------------------------------------------------------------
// Signing — lit-frost, 2 rounds. Each signer runs round1 (commit) then round2
// (sign share); the coordinator (the user) aggregates the two shares into one
// 64-byte Ed25519 signature.
// ---------------------------------------------------------------------------

/// Round 1: from this signer's signing share, produce a one-time nonce (secret)
/// + its commitment (public) + this signer's verifying share.
#[wasm_bindgen]
pub fn sign_round1(signing_share: Vec<u8>) -> Result<JsValue, JsValue> {
    let share = SigningShare::try_from(signing_share.as_slice()).map_err(err)?;
    let mut rng = rand_core::OsRng;
    let (nonce, commitment) = SCHEME.signing_round1(&share, &mut rng).map_err(err)?;
    let verifying_share = SCHEME.verifying_share(&share).map_err(err)?;
    to_js(&SignRound1Out {
        nonce: (&nonce).into(),
        commitment: (&commitment).into(),
        verifying_share: (&verifying_share).into(),
    })
}

/// Round 2: from the message, all signers' commitments, this signer's nonce and
/// key material, produce this signer's signature share.
#[wasm_bindgen]
pub fn sign_round2(
    message: Vec<u8>,
    my_id: u16,
    signing_share: Vec<u8>,
    verifying_key: Vec<u8>,
    threshold: u16,
    commitments: JsValue,
    nonce: Vec<u8>,
) -> Result<JsValue, JsValue> {
    let share = SigningShare::try_from(signing_share.as_slice()).map_err(err)?;
    let group_key = VerifyingKey::try_from(verifying_key.as_slice()).map_err(err)?;
    let nonce = SigningNonces::try_from(nonce.as_slice()).map_err(err)?;
    let commitments = decode_commitments(commitments)?;

    let key_package = KeyPackage {
        identifier: lit_id(my_id),
        secret_share: share.clone(),
        verifying_key: group_key,
        threshold: NonZeroU16::new(threshold).ok_or_else(|| err("threshold must be >= 1"))?,
    };
    let sig_share = SCHEME
        .signing_round2(&message, &commitments, &nonce, &key_package)
        .map_err(err)?;
    let verifying_share = SCHEME.verifying_share(&share).map_err(err)?;
    to_js(&SignRound2Out {
        signature_share: (&sig_share).into(),
        verifying_share: (&verifying_share).into(),
    })
}

/// Aggregate the signature shares into one 64-byte Ed25519 signature. Done by
/// the coordinator (the user); needs no secret material. The returned bytes are
/// submit-ready for Solana.
#[wasm_bindgen]
pub fn aggregate(
    message: Vec<u8>,
    verifying_key: Vec<u8>,
    commitments: JsValue,
    signature_shares: JsValue,
    verifying_shares: JsValue,
) -> Result<Vec<u8>, JsValue> {
    let group_key = VerifyingKey::try_from(verifying_key.as_slice()).map_err(err)?;
    let commitments = decode_commitments(commitments)?;

    let shares: Vec<IdItem> = from_js(signature_shares)?;
    let signature_shares = shares
        .iter()
        .map(|i| Ok((lit_id(i.id), SignatureShare::try_from(i.data.as_slice()).map_err(err)?)))
        .collect::<Result<Vec<_>, JsValue>>()?;

    let vshares: Vec<IdItem> = from_js(verifying_shares)?;
    let signer_pubkeys = vshares
        .iter()
        .map(|i| Ok((lit_id(i.id), VerifyingShare::try_from(i.data.as_slice()).map_err(err)?)))
        .collect::<Result<Vec<_>, JsValue>>()?;

    let signature = SCHEME
        .aggregate(
            &message,
            &commitments,
            &signature_shares,
            &signer_pubkeys,
            &group_key,
        )
        .map_err(err)?;
    // raw 64-byte Ed25519 signature
    Ok(signature.value)
}

/// Verify a 64-byte Ed25519 signature against the group key (sanity check).
#[wasm_bindgen]
pub fn verify(message: Vec<u8>, verifying_key: Vec<u8>, signature: Vec<u8>) -> Result<bool, JsValue> {
    let group_key = VerifyingKey::try_from(verifying_key.as_slice()).map_err(err)?;
    let sig = Signature {
        scheme: SCHEME,
        value: signature,
    };
    Ok(SCHEME.verify(&message, &group_key, &sig).is_ok())
}

/// Derive a verifying share from a signing share (used in recovery signing,
/// where the user holds both shares locally).
#[wasm_bindgen]
pub fn verifying_share(signing_share: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    let share = SigningShare::try_from(signing_share.as_slice()).map_err(err)?;
    let vs = SCHEME.verifying_share(&share).map_err(err)?;
    Ok((&vs).into())
}

fn decode_commitments(v: JsValue) -> Result<Vec<(Identifier, SigningCommitments)>, JsValue> {
    let items: Vec<IdItem> = from_js(v)?;
    items
        .iter()
        .map(|i| Ok((lit_id(i.id), SigningCommitments::try_from(i.data.as_slice()).map_err(err)?)))
        .collect()
}
