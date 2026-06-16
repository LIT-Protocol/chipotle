import type { ApprovalRow, ApprovalStore } from '../src/types';

/** In-memory ApprovalStore for tests. Also lets a test play the role of a
 *  malicious store admin by mutating `rows` directly. */
export function memStore(): ApprovalStore & { rows: Map<string, ApprovalRow> } {
  const rows = new Map<string, ApprovalRow>();
  return {
    rows,
    async insertPending(row) {
      rows.set(row.approvalId, { ...row });
    },
    async load(id) {
      const r = rows.get(id);
      return r ? { ...r } : null;
    },
    async recordSubmission(id, clicked, submittedOtp) {
      const r = rows.get(id);
      if (r && r.status === 'pending') {
        r.clicked = clicked;
        r.submittedOtp = submittedOtp;
      }
    },
    async finalizeConsume(id, attestation) {
      const r = rows.get(id);
      if (!r || r.status !== 'pending') return false;
      r.status = 'consumed';
      r.attestation = attestation;
      return true;
    },
    async markDenied(id) {
      const r = rows.get(id);
      if (r && r.status === 'pending') r.status = 'denied';
    },
  };
}

/** Deterministic, NON-cryptographic randomBytes for tests (counter-seeded). */
export function seededRandom(seed = 1): (n: number) => Uint8Array {
  let s = seed >>> 0;
  return (n: number) => {
    const out = new Uint8Array(n);
    for (let i = 0; i < n; i++) {
      s = (s * 1664525 + 1013904223) >>> 0;
      out[i] = s & 0xff;
    }
    return out;
  };
}
