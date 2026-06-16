/**
 * Neon-backed `ApprovalStore` over the serverless SQL-over-HTTP endpoint
 * (`fetch`, no TCP — works inside the action runtime). The store is UNTRUSTED:
 * correctness never depends on it being honest (see README). It provides
 * shared state that survives horizontal scaling and ping-pong deploys, which
 * the v1 in-memory store could not.
 *
 * NOTE: the exact Neon HTTP wire shape must be validated against live Neon on
 * the dev deploy — the seam below (`NeonQuery`) is the tested interface; the
 * `neonHttpQuery` transport is best-effort until that check.
 */

import type { ApprovalRow, ApprovalStore, FetchLike, RowStatus, Assurance } from './types';

/** Minimal executor: run a parameterized statement, get rows back. */
export type NeonQuery = (sql: string, params: unknown[]) => Promise<Record<string, unknown>[]>;

export const SCHEMA_SQL = `
create table if not exists lit_approvals (
  approval_id    text primary key,
  approver       text not null,
  assurance      text not null check (assurance in ('L1','L2')),
  request_hash   text not null default '',
  status         text not null default 'pending',
  otp_hmac       text not null default '',
  clicked        boolean not null default false,
  submitted_otp  text,
  attestation    text,
  created_at_ms  bigint not null,
  expires_at_ms  bigint not null
);
`;

function rowFromDb(r: Record<string, unknown>): ApprovalRow {
  return {
    approvalId: String(r.approval_id),
    approver: String(r.approver),
    assurance: String(r.assurance) as Assurance,
    requestHash: String(r.request_hash ?? ''),
    status: String(r.status) as RowStatus,
    otpHmac: String(r.otp_hmac ?? ''),
    clicked: r.clicked === true || r.clicked === 't' || r.clicked === 'true',
    submittedOtp: r.submitted_otp == null ? null : String(r.submitted_otp),
    attestation: r.attestation == null ? null : String(r.attestation),
    createdAtMs: Number(r.created_at_ms),
    expiresAtMs: Number(r.expires_at_ms),
  };
}

export function neonStore(query: NeonQuery): ApprovalStore {
  return {
    async insertPending(row: ApprovalRow): Promise<void> {
      await query(
        `insert into lit_approvals
           (approval_id, approver, assurance, request_hash, status, otp_hmac,
            clicked, submitted_otp, attestation, created_at_ms, expires_at_ms)
         values ($1,$2,$3,$4,'pending',$5,false,null,null,$6,$7)`,
        [row.approvalId, row.approver, row.assurance, row.requestHash, row.otpHmac, row.createdAtMs, row.expiresAtMs],
      );
    },
    async load(approvalId: string): Promise<ApprovalRow | null> {
      const rows = await query(`select * from lit_approvals where approval_id = $1`, [approvalId]);
      return rows[0] ? rowFromDb(rows[0]) : null;
    },
    async recordSubmission(approvalId, clicked, submittedOtp): Promise<void> {
      await query(
        `update lit_approvals set clicked = $2, submitted_otp = $3
           where approval_id = $1 and status = 'pending'`,
        [approvalId, clicked, submittedOtp],
      );
    },
    async finalizeConsume(approvalId, attestation): Promise<boolean> {
      // Atomic single-use: only the row still 'pending' transitions. Note an
      // adversary with DB write can roll this back — true anti-replay lives at
      // the execution layer (request_hash bound to a chain nonce / order id).
      const rows = await query(
        `update lit_approvals set status = 'consumed', attestation = $2
           where approval_id = $1 and status = 'pending'
         returning approval_id`,
        [approvalId, attestation],
      );
      return rows.length === 1;
    },
    async markDenied(approvalId): Promise<void> {
      await query(`update lit_approvals set status = 'denied' where approval_id = $1 and status = 'pending'`, [approvalId]);
    },
  };
}

/**
 * Best-effort Neon serverless SQL-over-HTTP transport. The connection string
 * is a full-access credential — seal it (do NOT pass it as a plaintext action
 * param). Validate the exact header/response shape against live Neon before GA.
 */
export function neonHttpQuery(connectionString: string, fetchImpl: FetchLike): NeonQuery {
  const host = connectionString.replace(/^postgres(ql)?:\/\//, '').split('@')[1]?.split('/')[0]?.split('?')[0];
  if (!host) throw new Error('lit-approvals: cannot parse host from Neon connection string');
  const endpoint = `https://${host}/sql`;
  return async (sql, params) => {
    const res = await fetchImpl(endpoint, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Neon-Connection-String': connectionString,
        'Neon-Raw-Text-Output': 'false',
        'Neon-Array-Mode': 'false',
      },
      body: JSON.stringify({ query: sql, params }),
    });
    const text = await res.text();
    if (!res.ok) throw new Error(`lit-approvals: Neon HTTP ${res.status}: ${text.slice(0, 200)}`);
    const parsed = JSON.parse(text) as { rows?: Record<string, unknown>[] };
    return parsed.rows ?? [];
  };
}
