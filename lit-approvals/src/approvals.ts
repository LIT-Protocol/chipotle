/**
 * The email-approval primitive, as action logic (no server). Three entry
 * points map to the two-phase flow:
 *
 *   requestApproval()  — phase 1, in-TEE: issue nonce + OTP, persist a pending
 *                        row to the (untrusted) shared store, send the email.
 *   recordSubmission() — out-of-TEE, on the approval page (flows): record that
 *                        the human clicked / typed an OTP. Cannot approve.
 *   checkApproval()    — phase 2, in-TEE: validate the human action against the
 *                        TEE-keyed OTP HMAC, then SIGN the attestation with the
 *                        action's CID-bound key and consume the row.
 *   verifyApproval()   — in the consuming action, in-TEE: verify the signed
 *                        attestation against the pinned signer pubkey AND bind
 *                        it to the operation's request_hash. Fails closed.
 */

import {
  deriveOtpKey,
  genApprovalId,
  genOtp,
  otpHmacHex,
  publicKeyHex,
  signPayload,
  timingSafeEqualHex,
  verifyPayloadSig,
  type RandomBytes,
  defaultRandomBytes,
} from './crypto';
import type {
  ApprovalStore,
  Assurance,
  AttestationEnvelope,
  AttestationPayload,
  FetchLike,
} from './types';

function nowMsOf(nowMs?: number): number {
  return nowMs ?? Date.now();
}

/** Canonical, fixed-key-order serialization — the EXACT bytes that are signed
 *  and that the verifier re-parses. */
function serializePayload(p: AttestationPayload): string {
  return JSON.stringify({
    schema: p.schema,
    approval_id: p.approval_id,
    approver: p.approver,
    assurance: p.assurance,
    request_hash: p.request_hash,
    status: p.status,
    approved_at_ms: p.approved_at_ms,
    expires_at_ms: p.expires_at_ms,
  });
}

export interface RequestApprovalInput {
  store: ApprovalStore;
  /** The action's CID-bound private key (`Lit.Actions.getLitActionPrivateKey()`). */
  signingKey: string | Uint8Array;
  to: string;
  /** Human-readable operation summary for the email body. */
  summary: string;
  assurance: Assurance;
  ttlSec: number;
  /** Operation binding (e.g. sha256 of amount|dest|venue|nonce). Empty for L1. */
  requestHash?: string;
  /** Base URL of the approval page (flows). The link is `${base}/${approvalId}`. */
  approvalBaseUrl: string;
  sendEmail: (msg: { to: string; subject: string; text: string }) => Promise<void>;
  nowMs?: number;
  randomBytes?: RandomBytes;
}

export interface RequestApprovalResult {
  approvalId: string;
  /** Present for L2 — returned to the REQUESTING app (email is notification
   *  only). undefined for L1. */
  otp?: string;
  approvalUrl: string;
}

export async function requestApproval(input: RequestApprovalInput): Promise<RequestApprovalResult> {
  const rand = input.randomBytes ?? defaultRandomBytes;
  if (input.assurance === 'L2' && !input.requestHash) {
    throw new Error('lit-approvals: L2 (fund-moving) approval requires a requestHash');
  }
  const approvalId = genApprovalId(rand);
  const now = nowMsOf(input.nowMs);
  const otp = input.assurance === 'L2' ? genOtp(rand) : '';
  const otpKey = deriveOtpKey(input.signingKey);
  const otpHmac = otp ? otpHmacHex(otpKey, approvalId, otp) : '';

  await input.store.insertPending({
    approvalId,
    approver: input.to,
    assurance: input.assurance,
    requestHash: input.requestHash ?? '',
    status: 'pending',
    otpHmac,
    clicked: false,
    submittedOtp: null,
    attestation: null,
    createdAtMs: now,
    expiresAtMs: now + input.ttlSec * 1000,
  });

  const approvalUrl = `${input.approvalBaseUrl.replace(/\/$/, '')}/${approvalId}`;
  await input.sendEmail({
    to: input.to,
    subject: 'Approval requested',
    text: `${input.summary}\n\nReview and approve: ${approvalUrl}\n\nThis request expires in ${Math.round(input.ttlSec / 60)} minutes. If you did not initiate it, ignore this email.`,
  });

  return { approvalId, otp: otp || undefined, approvalUrl };
}

/** Out-of-TEE recorder used by the approval page (flows). It can never produce
 *  an approval — only the in-TEE `checkApproval` validates + signs. */
export async function recordSubmission(
  store: ApprovalStore,
  approvalId: string,
  opts: { clicked: boolean; submittedOtp?: string | null },
): Promise<void> {
  await store.recordSubmission(approvalId, opts.clicked, opts.submittedOtp ?? null);
}

export interface CheckApprovalInput {
  store: ApprovalStore;
  signingKey: string | Uint8Array;
  approvalId: string;
  nowMs?: number;
}

export interface CheckApprovalResult {
  approved: boolean;
  /** 'pending' | 'approved' | 'denied' | 'expired' | 'replayed' */
  status: string;
  attestation?: string;
}

/**
 * Validate the human action against the TEE-keyed OTP HMAC and, if valid, mint
 * + persist + return the signed attestation. Single-use via the store's atomic
 * `finalizeConsume` (fails closed if it lost the race).
 */
export async function checkApproval(input: CheckApprovalInput): Promise<CheckApprovalResult> {
  const row = await input.store.load(input.approvalId);
  if (!row) return { approved: false, status: 'pending' };
  if (row.status === 'denied') return { approved: false, status: 'denied' };
  if (row.status === 'consumed') {
    // Already spent: return the prior attestation, but the caller must treat a
    // second consume as a replay at the execution layer (request_hash → nonce).
    return { approved: false, status: 'replayed', attestation: row.attestation ?? undefined };
  }
  const now = nowMsOf(input.nowMs);
  if (now >= row.expiresAtMs) return { approved: false, status: 'expired' };
  if (!row.clicked) return { approved: false, status: 'pending' };

  if (row.assurance === 'L2') {
    const otpKey = deriveOtpKey(input.signingKey);
    const submitted = row.submittedOtp ?? '';
    if (!submitted || !timingSafeEqualHex(otpHmacHex(otpKey, input.approvalId, submitted), row.otpHmac)) {
      return { approved: false, status: 'pending' };
    }
  }

  const payload: AttestationPayload = {
    schema: 'email-approval-v1',
    approval_id: row.approvalId,
    approver: row.approver,
    assurance: row.assurance,
    request_hash: row.requestHash,
    status: 'approved',
    approved_at_ms: now,
    expires_at_ms: row.expiresAtMs,
  };
  const payloadStr = serializePayload(payload);
  const envelope: AttestationEnvelope = {
    v: 'email-approval-v1',
    alg: 'secp256k1-sha256',
    payload: payloadStr,
    sig: signPayload(payloadStr, input.signingKey),
  };
  const attestation = JSON.stringify(envelope);

  const won = await input.store.finalizeConsume(input.approvalId, attestation);
  if (!won) return { approved: false, status: 'replayed' };
  return { approved: true, status: 'approved', attestation };
}

export class ApprovalVerifyError extends Error {}

export interface VerifyApprovalInput {
  attestation: string;
  approvalId: string;
  /** Operation the consuming action is about to perform. Must match the bound
   *  hash; if the action expects a bound approval it MUST pass non-empty. */
  expectedRequestHash: string;
  /** SEC1 pubkey hex of the approval action's signing key (pin this). */
  signerPubKeyHex: string;
  nowMs?: number;
}

/**
 * Mirror of the runtime's `verify_approval_attestation`, in action JS (still
 * in-TEE). Every failure is terminal — an approval that cannot prove THIS
 * operation reports an error, never `approved`.
 */
export function verifyApproval(input: VerifyApprovalInput): AttestationPayload {
  let envelope: AttestationEnvelope;
  try {
    envelope = JSON.parse(input.attestation) as AttestationEnvelope;
  } catch {
    throw new ApprovalVerifyError('malformed attestation');
  }
  if (envelope.v !== 'email-approval-v1' || envelope.alg !== 'secp256k1-sha256') {
    throw new ApprovalVerifyError(`unsupported attestation version/alg: ${envelope.v}/${envelope.alg}`);
  }
  if (!verifyPayloadSig(envelope.payload, envelope.sig, input.signerPubKeyHex)) {
    throw new ApprovalVerifyError('attestation signature verification FAILED');
  }
  let payload: AttestationPayload;
  try {
    payload = JSON.parse(envelope.payload) as AttestationPayload;
  } catch {
    throw new ApprovalVerifyError('malformed attestation payload');
  }
  if (payload.schema !== 'email-approval-v1') {
    throw new ApprovalVerifyError(`unexpected payload schema: ${payload.schema}`);
  }
  if (payload.approval_id !== input.approvalId) {
    throw new ApprovalVerifyError('attestation is for a different approvalId');
  }
  if (payload.status !== 'approved') {
    throw new ApprovalVerifyError(`attestation status is '${payload.status}', not 'approved'`);
  }
  if (payload.request_hash !== input.expectedRequestHash) {
    throw new ApprovalVerifyError(
      payload.request_hash === ''
        ? 'attestation is unbound (no operation hash) but this action expects a bound approval — refusing'
        : 'attestation does not match the expected operation (requestHash mismatch)',
    );
  }
  if (nowMsOf(input.nowMs) >= payload.expires_at_ms) {
    throw new ApprovalVerifyError('attestation has expired');
  }
  return payload;
}

export { publicKeyHex };
