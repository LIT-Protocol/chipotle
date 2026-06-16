# @lit-protocol/lit-approvals

The email-approval primitive (plan D6/M3) re-implemented as **action logic** —
no `ApprovalService` in lit-api-server. State lives in an **untrusted shared
store** (Neon over HTTP), the attestation is **signed in-TEE with the approval
action's CID-bound key**, and verification is **in-TEE action JS**. This fixes
the v1 in-memory store, which dropped pending approvals on every horizontal
scale-out / ping-pong deploy (plan D6.1).

> Library code only — like `lit-venues`, it bundles to an IIFE for inlining
> into a Lit Action. It is reachable from the runtime because Neon's serverless
> SQL endpoint and Resend are plain HTTPS (`fetch` / `Lit.Actions.proxiedFetch`).

## Architecture

```
phase 1  (approval ACTION, in-TEE)   requestApproval()  → issue id+OTP, write pending row to Neon, email link
   ↓
human    (approval PAGE, flows)       recordSubmission() → record click + typed OTP   (cannot approve)
   ↓
phase 2  (approval ACTION, in-TEE)   checkApproval()    → verify OTP vs TEE-keyed HMAC, SIGN attestation, consume
   ↓
consume  (consuming ACTION, in-TEE)  verifyApproval()   → check signature + approvalId + request_hash + expiry
```

One CID-bound key (`Lit.Actions.getLitActionPrivateKey()`) powers both the OTP
HMAC (domain-separated) and the secp256k1 attestation signature. Consuming
actions pin the signer pubkey (`publicKeyHex(...)`) — the analog of the old
`LIT_APPROVAL_ATTESTATION_PUBKEY`, but now a CID-bound identity no service holds.

## Threat model — what an untrusted store CANNOT do

Assume the adversary can **read and write every Neon row** (malicious DB admin,
leaked connection string, Neon itself):

- **Cannot forge an approval.** Approval is proven by the secp256k1 signature
  over `(approvalId, requestHash, status, expiry)`. The adversary doesn't hold
  the signing key, so any row they fabricate fails `verifyApproval`. *(Tested:
  "malicious store admin", "signer key isolation".)*
- **Cannot substitute a real attestation onto another operation/approval.** The
  signed payload binds `approvalId` and `request_hash`; a mismatch is rejected.
  This is why naïve "encrypt-and-compare" was rejected — encryption isn't
  authentication. *(Tested: "operation binding", "ciphertext/attestation
  substitution".)*
- **Cannot brute-force the OTP from the store.** Only `HMAC(otpKey, id:otp)` is
  stored; `otpKey` is derived in-TEE and never persisted, so the low-entropy
  6-digit OTP can't be recovered or guessed offline.
- **Can deny / DoS** (delete rows, withhold). That's availability, not
  integrity — acceptable and unavoidable for any shared store.

**Single-use** is enforced atomically on consume (`finalizeConsume`), but an
adversary with DB write can roll a `consumed` flag back. True anti-replay must
therefore live at the **execution layer**: bind `request_hash` to the
operation's own idempotency key (chain nonce, venue `clientOrderId`) so a
replayed approval still can't execute twice.

**L1 vs L2.** L1 (link-click only) is notification-grade and unbound — a store
adversary *can* fake an L1 click, so L1 must never gate fund movement.
`verifyApproval` refuses an unbound attestation wherever a `request_hash` is
expected. L2 (OTP step-up) is required for money movement and is what the
threat-model guarantees above protect.

## Build

```sh
npm install
npm test         # vitest — threat model is encoded as tests
npm run build    # esbuild → dist/lit-approvals.iife.js (+ .mjs)
npm run typecheck
```

## Usage sketch (inside the approval action)

```js
// dist/lit-approvals.iife.js concatenated above → global `LitApprovals`
const signingKey = await Lit.Actions.getLitActionPrivateKey();
const store = LitApprovals.neonStore(LitApprovals.neonHttpQuery(neonConnStr, fetch));

// phase 1
const { approvalId, otp } = await LitApprovals.requestApproval({
  store, signingKey, to: user.email, summary: 'Sweep 0.1 BTC to cold storage',
  assurance: 'L2', ttlSec: 600,
  requestHash: LitApprovals.sha256Hex(`${amount}|${dest}|${venue}|${nonce}`),
  approvalBaseUrl: 'https://flows.litprotocol.com/approvals',
  sendEmail: (m) => sendViaResend(resendKey, m),  // action calls Resend directly
});
// return otp to the requesting app (email is notification only)

// phase 2 (next tick / webhook / manual re-invoke)
const { approved, attestation } = await LitApprovals.checkApproval({ store, signingKey, approvalId });

// consuming action
const payload = LitApprovals.verifyApproval({
  attestation, approvalId, expectedRequestHash, signerPubKeyHex,  // pinned
});
```

The connection string is a full-access credential — **seal it** (don't pass it
as a plaintext action param). The exact Neon HTTP wire shape (`neonHttpQuery`)
must be validated against live Neon on the dev deploy; the tested seam is the
`NeonQuery` executor.
