# CEX Sweep with Email Approval

**A CEX → self-custody sweep that cannot happen until a human approves it —
and where the approval cannot be forged, even by the approval server itself.
Phase 1 applies the policy and emails the approver; phase 2 checks the
approval attestation, verified in-TEE by the action runtime, and only then
performs the gated step.** This is the two-phase pattern from plan D6.

## The two-phase pattern

Lit Actions are request-scoped (15-minute cap), so "wait for a human" is
structurally two actions, not one long one:

```
request-sweep.js          requestSweep action            approval server     approver
     │                          │                              │                │
     │ creds + intent ─────────►│ fetchBalances (testnet)      │                │
     │                          │ policy: balance covers it?   │                │
     │                          │ requestEmailApproval(L2) ───►│ ─ email link ─►│
     │◄─ {approvalId, otp} ─────┤ (action exits)               │                │
     │ ······ hand the OTP to the approver OUT-OF-BAND ······························►│
     │                          │                              │◄─ link + OTP ──┤
complete-sweep.js         completeSweep action                 │ sign attestation
     │ approvalId ─────────────►│ checkEmailApproval ─────────►│                │
     │                          │ runtime verifies the         │                │
     │◄─ {swept, attestation} ──┤ attestation IN-TEE, then     │                │
     │                          │ performs the gated step      │                │
```

If the approver hasn't decided yet, phase 2 returns `{swept:false,
status:"pending"}` — re-run it later, or in production let a
[lit-triggers](../lit-triggers) approval-completed webhook re-invoke it.

## Assurance levels (what "approved" is worth)

- **L1** — link click. Low-stakes confirms ("run the weekly report").
- **L2** — link click **+ one-time code**, where the code reaches the approver
  via the requesting app, never in the email. Email is the *notification*
  channel, not the *authentication* channel: a hijacked inbox alone can't
  approve. Required default for anything that moves funds — **this example**.
- **L3** — EIP-712/EIP-1271 co-sign (EOA or Safe) for treasury-grade moves.

The assurance level is baked into `POLICY` in the action source, so it is part
of the action's CID: a caller cannot quietly downgrade L2 to L1.

## Why in-TEE verification matters

`checkEmailApproval` doesn't trust the approval server's word. The approval is
delivered as an **attestation signed by the network attestation key**, and the
action runtime verifies the signature, the `approvalId` binding, the status,
and the expiry *inside the TEE* before it ever reports `approved: true`. A
fully compromised approval server — or a malicious caller invoking phase 2 —
can delay an approval, but cannot forge one. The action only branches on
`approved`; everything load-bearing already happened below it.

## What "sweep" means in this demo

lit-venues deliberately exposes **no withdrawal endpoints** (a v1 non-goal:
policy-gated sweeps go through this approval primitive first). So phase 1's
"sweep" is the real policy decision (balance must cover the intent, L2
approval required, intent recorded in the human-readable summary), and phase
2's is the post-approval verification plus the attestation as the audit
record. In production, phase 2 is exactly where the action signs the venue's
withdrawal request or an on-chain transfer — same gate, real movement.

## Files

| Path | Purpose |
| --- | --- |
| `action/requestSweep.js` | Phase 1. Reads Binance spot-testnet balances via `LitVenues`, applies the policy, requests the L2 approval, exits. |
| `action/completeSweep.js` | Phase 2. `checkEmailApproval` (attestation verified in-TEE), then performs the gated step and returns the attestation. |
| `scripts/_lit.js` | Concatenates the prebuilt `lit-venues` IIFE bundle (~175 KB) above the action source and runs it via `/lit_action`. |
| `scripts/_env.js` | Minimal `.env` reader / upserter, inlined so the folder is self-contained. |
| `scripts/setup.js` | One-shot: CIDs for both actions, permission group, scoped usage key, registration, readiness probe. |
| `scripts/request-sweep.js` | Phase 1 driver — prints the OTP (the out-of-band factor) and the approval link when the server exposes it. |
| `scripts/complete-sweep.js` | Phase 2 driver — completes, or reports pending/denied/expired. |

## Walkthrough

### 1. Install + configure

```bash
cp .env.example .env
npm install
```

Set in `.env`: `LIT_API_KEY` (account-level key), `APPROVER_EMAIL`,
`BINANCE_TESTNET_API_KEY` / `BINANCE_TESTNET_SECRET` (free at
<https://testnet.binance.vision>, GitHub login, pre-funded play assets), and
`SWEEP_DESTINATION`. The action code lives next to a built
`../../lit-venues/dist/lit-venues.iife.js` (committed; rebuild with
`npm run build` in `lit-venues/` if needed).

### 2. Setup, request, approve, complete

```bash
npm run setup
npm run request-sweep -- 100 USDT
```

Phase 1 prints the **approval id**, the **one-time code**, and — on a dev
server running with `LIT_APPROVAL_EXPOSE_LINK` — the **approval link**.
Without that flag (the production posture) the link goes only to the
approver's inbox, so use an `APPROVER_EMAIL` you can read. Open the link,
enter the code, approve. A wrong code is refused and the approval stays
pending; the right one is single-use. Then:

```bash
npm run complete-sweep
```

Run it *before* approving to see the honest `pending` path. Deny the request
at the page to see `denied` (final — request again).

## Prerequisites, honestly

- **Email approval ops** (`requestEmailApproval` / `checkEmailApproval`) need
  a Lit environment with the D6 approval service and the runtime's attestation
  pubkey pinned — the dev environment has both. Phase 2 fails closed if the
  pin is missing.
- **The clickable demo loop** (no inbox needed) additionally needs
  `LIT_APPROVAL_EXPOSE_LINK` on the server — dev-only by design.
- **Binance geo-blocks US egress** (HTTP 451, testnet included). If your Lit
  environment egresses from the US, set `VENUE_PROXY_URL` and the action
  routes venue calls through the in-TEE `Lit.Actions.proxiedFetch` op (D4).

## Production notes

- **Seal the credentials.** Here venue keys travel as `js_params` to keep the
  demo honest about what's demo. Production stores them as a sealed
  `venue-credentials-v1` record and decrypts in-TEE
  (`Lit.Actions.Decrypt`) — the caller never handles plaintext keys.
- **Bind intent to approval.** The summary the human approves contains the
  exact amount/asset/destination; keep phase-2 inputs bound to it (e.g. store
  the intent against `approvalId` in flow storage, or derive both phases from
  one source) so what was approved is what executes.
- **Automate phase 2** with a lit-triggers approval-completed webhook, and
  step up to **L3** (EIP-712/Safe co-sign) for treasury-grade sweeps.
