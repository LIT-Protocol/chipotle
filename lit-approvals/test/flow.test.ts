import { describe, it, expect } from 'vitest';
import {
  requestApproval,
  recordSubmission,
  checkApproval,
  verifyApproval,
  publicKeyHex,
  ApprovalVerifyError,
} from '../src/approvals';
import { sha256Hex, signPayload } from '../src/crypto';
import { memStore, seededRandom } from './memstore';

// A fixed 32-byte signing key standing in for getLitActionPrivateKey().
const SIGNING_KEY = '11'.repeat(32);
const PUB = publicKeyHex(SIGNING_KEY);
const T0 = 1_700_000_000_000;

function noopEmail() {
  const sent: { to: string; subject: string; text: string }[] = [];
  return { sent, send: async (m: { to: string; subject: string; text: string }) => void sent.push(m) };
}

async function l2Request(store = memStore(), requestHash = sha256Hex('move 0.1 BTC to 0xabc|nonce42'), seed = 7) {
  const email = noopEmail();
  const res = await requestApproval({
    store,
    signingKey: SIGNING_KEY,
    to: 'user@example.com',
    summary: 'Sweep 0.1 BTC to cold storage',
    assurance: 'L2',
    ttlSec: 600,
    requestHash,
    approvalBaseUrl: 'https://flows.example/approvals',
    sendEmail: email.send,
    nowMs: T0,
    randomBytes: seededRandom(seed),
  });
  return { store, res, requestHash, email };
}

describe('happy path (L2)', () => {
  it('request → human OTP submit → check signs an attestation that verifies', async () => {
    const { store, res, requestHash } = await l2Request();
    expect(res.otp).toMatch(/^\d{6}$/);
    expect(res.approvalUrl).toContain(res.approvalId);

    // human acts on the (untrusted) page
    await recordSubmission(store, res.approvalId, { clicked: true, submittedOtp: res.otp });

    const checked = await checkApproval({ store, signingKey: SIGNING_KEY, approvalId: res.approvalId, nowMs: T0 + 1000 });
    expect(checked.approved).toBe(true);
    expect(checked.attestation).toBeTruthy();

    const payload = verifyApproval({
      attestation: checked.attestation!,
      approvalId: res.approvalId,
      expectedRequestHash: requestHash,
      signerPubKeyHex: PUB,
      nowMs: T0 + 2000,
    });
    expect(payload.status).toBe('approved');
    expect(payload.request_hash).toBe(requestHash);
  });

  it('emails a notification with the link, never the OTP', async () => {
    const { res, email } = await l2Request();
    expect(email.sent).toHaveLength(1);
    expect(email.sent[0]!.text).toContain(res.approvalUrl);
    expect(email.sent[0]!.text).not.toContain(res.otp!);
  });
});

describe('operation binding', () => {
  it('rejects a valid attestation replayed for a DIFFERENT operation', async () => {
    const { store, res } = await l2Request();
    await recordSubmission(store, res.approvalId, { clicked: true, submittedOtp: res.otp });
    const checked = await checkApproval({ store, signingKey: SIGNING_KEY, approvalId: res.approvalId, nowMs: T0 + 1000 });
    expect(() =>
      verifyApproval({
        attestation: checked.attestation!,
        approvalId: res.approvalId,
        expectedRequestHash: sha256Hex('move 5 BTC to 0xEVIL'),
        signerPubKeyHex: PUB,
        nowMs: T0 + 2000,
      }),
    ).toThrow(/does not match the expected operation/);
  });

  it('refuses an unbound (L1) approval where a bound one is expected', async () => {
    const store = memStore();
    const email = noopEmail();
    const res = await requestApproval({
      store, signingKey: SIGNING_KEY, to: 'u@e.com', summary: 'run report',
      assurance: 'L1', ttlSec: 600, approvalBaseUrl: 'https://flows.example/approvals',
      sendEmail: email.send, nowMs: T0, randomBytes: seededRandom(3),
    });
    await recordSubmission(store, res.approvalId, { clicked: true });
    const checked = await checkApproval({ store, signingKey: SIGNING_KEY, approvalId: res.approvalId, nowMs: T0 + 1000 });
    expect(checked.approved).toBe(true); // L1 link-click approves
    expect(() =>
      verifyApproval({
        attestation: checked.attestation!, approvalId: res.approvalId,
        expectedRequestHash: sha256Hex('move funds'), signerPubKeyHex: PUB, nowMs: T0 + 2000,
      }),
    ).toThrow(/unbound/);
  });

  it('requires a requestHash for L2 at request time', async () => {
    await expect(
      requestApproval({
        store: memStore(), signingKey: SIGNING_KEY, to: 'u@e.com', summary: 's',
        assurance: 'L2', ttlSec: 600, approvalBaseUrl: 'https://flows.example/approvals',
        sendEmail: noopEmail().send, nowMs: T0, randomBytes: seededRandom(1),
      }),
    ).rejects.toThrow(/requires a requestHash/);
  });
});

describe('OTP step-up', () => {
  it('does not approve on a wrong OTP', async () => {
    const { store, res } = await l2Request();
    await recordSubmission(store, res.approvalId, { clicked: true, submittedOtp: '000000' });
    const checked = await checkApproval({ store, signingKey: SIGNING_KEY, approvalId: res.approvalId, nowMs: T0 + 1000 });
    expect(checked.approved).toBe(false);
    expect(checked.attestation).toBeUndefined();
  });

  it('does not approve on click without an OTP (L2)', async () => {
    const { store, res } = await l2Request();
    await recordSubmission(store, res.approvalId, { clicked: true });
    const checked = await checkApproval({ store, signingKey: SIGNING_KEY, approvalId: res.approvalId, nowMs: T0 + 1000 });
    expect(checked.approved).toBe(false);
  });
});

describe('malicious store admin (the whole point)', () => {
  it('cannot forge an approval by writing status/attestation directly', async () => {
    const { store, res, requestHash } = await l2Request();
    // Adversary with full DB write sets the row "approved" and injects junk.
    const row = store.rows.get(res.approvalId)!;
    row.status = 'pending';
    row.clicked = true;
    row.submittedOtp = '000000'; // they don't know the real OTP
    row.attestation = JSON.stringify({ v: 'email-approval-v1', alg: 'secp256k1-sha256', payload: '{}', sig: '00'.repeat(64) });

    // The legit check still won't approve (OTP HMAC mismatch — they lack otpKey).
    const checked = await checkApproval({ store, signingKey: SIGNING_KEY, approvalId: res.approvalId, nowMs: T0 + 1000 });
    expect(checked.approved).toBe(false);

    // And the consuming verifier rejects any attestation they fabricate.
    expect(() =>
      verifyApproval({
        attestation: row.attestation!, approvalId: res.approvalId,
        expectedRequestHash: requestHash, signerPubKeyHex: PUB, nowMs: T0 + 1000,
      }),
    ).toThrow(ApprovalVerifyError);
  });

  it('ciphertext/attestation substitution: a valid attestation for a different approval is rejected', async () => {
    const a = await l2Request(undefined, sha256Hex('op-A'), 7);
    await recordSubmission(a.store, a.res.approvalId, { clicked: true, submittedOtp: a.res.otp });
    const goodA = await checkApproval({ store: a.store, signingKey: SIGNING_KEY, approvalId: a.res.approvalId, nowMs: T0 + 1000 });

    // Adversary drops attestation A onto a fresh pending approval B (distinct id).
    const b = await l2Request(undefined, sha256Hex('op-B'), 99);
    expect(() =>
      verifyApproval({
        attestation: goodA.attestation!, approvalId: b.res.approvalId,
        expectedRequestHash: b.requestHash, signerPubKeyHex: PUB, nowMs: T0 + 1000,
      }),
    ).toThrow(/different approvalId/);
  });
});

describe('single-use + expiry', () => {
  it('consumes once; a second check reports replayed, not approved', async () => {
    const { store, res } = await l2Request();
    await recordSubmission(store, res.approvalId, { clicked: true, submittedOtp: res.otp });
    const first = await checkApproval({ store, signingKey: SIGNING_KEY, approvalId: res.approvalId, nowMs: T0 + 1000 });
    expect(first.approved).toBe(true);
    const second = await checkApproval({ store, signingKey: SIGNING_KEY, approvalId: res.approvalId, nowMs: T0 + 1100 });
    expect(second.approved).toBe(false);
    expect(second.status).toBe('replayed');
  });

  it('rejects an expired pending approval, and an expired attestation', async () => {
    const { store, res, requestHash } = await l2Request();
    await recordSubmission(store, res.approvalId, { clicked: true, submittedOtp: res.otp });
    const late = await checkApproval({ store, signingKey: SIGNING_KEY, approvalId: res.approvalId, nowMs: T0 + 10_000_000 });
    expect(late.approved).toBe(false);
    expect(late.status).toBe('expired');

    // And even a genuine attestation fails verification past its expiry.
    const store2 = memStore();
    const r2 = await l2Request(store2, requestHash);
    await recordSubmission(store2, r2.res.approvalId, { clicked: true, submittedOtp: r2.res.otp });
    const ok = await checkApproval({ store: store2, signingKey: SIGNING_KEY, approvalId: r2.res.approvalId, nowMs: T0 + 1000 });
    expect(() =>
      verifyApproval({
        attestation: ok.attestation!, approvalId: r2.res.approvalId,
        expectedRequestHash: requestHash, signerPubKeyHex: PUB, nowMs: T0 + 10_000_000,
      }),
    ).toThrow(/expired/);
  });
});

describe('signer key isolation', () => {
  it('an attestation signed by a DIFFERENT key does not verify against the pinned pubkey', async () => {
    const { res, requestHash } = await l2Request();
    // A malicious sibling action forges its OWN well-formed attestation, signed
    // with its own CID-bound key (which is NOT the approval action's key).
    const payload = JSON.stringify({
      schema: 'email-approval-v1', approval_id: res.approvalId, approver: 'user@example.com',
      assurance: 'L2', request_hash: requestHash, status: 'approved',
      approved_at_ms: T0, expires_at_ms: T0 + 600_000,
    });
    const forged = JSON.stringify({
      v: 'email-approval-v1', alg: 'secp256k1-sha256', payload,
      sig: signPayload(payload, '22'.repeat(32)),
    });
    expect(() =>
      verifyApproval({
        attestation: forged, approvalId: res.approvalId,
        expectedRequestHash: requestHash, signerPubKeyHex: PUB, nowMs: T0 + 2000,
      }),
    ).toThrow(/signature verification FAILED/);
  });
});
