export type Assurance = 'L1' | 'L2';

/** DB row status. Distinct from the *attestation* payload status (which is
 *  always 'approved' when present). `consumed` is the single-use terminal. */
export type RowStatus = 'pending' | 'consumed' | 'denied' | 'expired';

export type FetchLike = (url: string, init?: Record<string, unknown>) => Promise<{
  status: number;
  ok: boolean;
  text(): Promise<string>;
}>;

/** A pending approval as persisted in the untrusted shared store. Nothing here
 *  is trusted for integrity: forgery is prevented by the signature the runtime
 *  verifies, and OTP confidentiality by `otpHmac` being keyed with a TEE-held
 *  key. See README "Threat model". */
export interface ApprovalRow {
  approvalId: string;
  approver: string;
  assurance: Assurance;
  /** Operation binding. Empty = unbound (L1 notification grade). */
  requestHash: string;
  status: RowStatus;
  /** HMAC(otpKey, `${approvalId}:${otp}`) — otpKey is derived in-TEE, never
   *  stored. A store adversary can't reverse this to recover the 6-digit OTP. */
  otpHmac: string;
  /** Set by the (untrusted) approval page when the human acts. */
  clicked: boolean;
  submittedOtp: string | null;
  /** Signed `email-approval-v1` envelope, written once on consume. */
  attestation: string | null;
  createdAtMs: number;
  expiresAtMs: number;
}

/** The shared-store seam. The Neon implementation is one impl; tests use an
 *  in-memory one. Integrity does NOT depend on this store being honest. */
export interface ApprovalStore {
  insertPending(row: ApprovalRow): Promise<void>;
  load(approvalId: string): Promise<ApprovalRow | null>;
  /** Called from the untrusted approval page (flows) when the human acts. */
  recordSubmission(approvalId: string, clicked: boolean, submittedOtp: string | null): Promise<void>;
  /** Atomic single-use guard: set status=consumed + store attestation ONLY if
   *  the row is still `pending`. Returns false if it lost the race / already
   *  consumed — the caller then fails closed. */
  finalizeConsume(approvalId: string, attestation: string): Promise<boolean>;
  markDenied(approvalId: string): Promise<void>;
}

/** The exact object that is JSON-serialized and signed. Field set + snake_case
 *  keys match the runtime verifier (`ApprovalAttestationPayload`) so the same
 *  attestation verifies in the Rust op or in action JS. */
export interface AttestationPayload {
  schema: 'email-approval-v1';
  approval_id: string;
  approver: string;
  assurance: Assurance;
  request_hash: string;
  status: 'approved';
  approved_at_ms: number;
  expires_at_ms: number;
}

export interface AttestationEnvelope {
  v: 'email-approval-v1';
  alg: 'secp256k1-sha256';
  payload: string;
  sig: string;
}
