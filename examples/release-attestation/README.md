# Release Attestation

**A GitHub release webhook, anchored on-chain by a keyless signer.** A
[lit-triggers](https://triggers.litprotocol.com) webhook trigger receives
GitHub's `release` event, verifies its `X-Hub-Signature-256` HMAC over the raw
body, and — only for genuine, signed, *published* releases — writes
`(repo, tag, commitish)` to a `ReleaseRegistry` contract. The transaction is
signed by a wallet derived from the action's IPFS CID, so the registry trusts
*this exact action code* and nothing else. No server holds the key; no one can
forge a release record.

Swap the "release published → registry" body for "Stripe
`customer.subscription.created` → `setSubscriber(addr, expiry)`" and you have
the web2-billing-unlocks-web3-access bridge — same skeleton, same HMAC verify.

## How it works

```
   GitHub repo            lit-triggers (webhook)        Lit network          ReleaseRegistry (Base Sepolia)
   ───────────            ──────────────────────        ───────────          ─────────────────────────────
   release published ─┐
   X-Hub-Signature-256 │  POST /webhook/<id>
                       ├────────────────────────►  run: main(params)
                       │  (raw body + headers)         │ verify HMAC(secret, event_raw)
                       │                               │ event=release / action=published?
                       │                               │ sign + send attest() ───────────► attest(repo,tag,commitish)
                       │                               │   from action wallet               require(msg.sender == attester) ✓
                       │   202 + run id                │                                     store + emit Attested
                       │◄───────────────────────       ▼
                                                   run history
```

### Why this is a Lit-shaped problem

A smart contract can't verify a GitHub webhook, and an off-chain script that
could would need to hold a signing key — whoever holds it can forge release
records. Here the signer key is derived from the action's IPFS CID
(`Lit.Actions.getLitActionPrivateKey`), so the trust assumption is "this exact,
content-addressed action verifies the GitHub signature before it writes." Edit
the action by a byte and its CID changes, its wallet changes, and the
`ReleaseRegistry` (which pins the original wallet as `attester`) rejects it.

The action **broadcasts the transaction itself**, rather than signing an
authorization for a caller to submit. That's because a trigger has no
downstream caller — the webhook delivery is the end of the line. So the action
wallet needs gas; `setup` funds it.

## Files

| Path | Purpose |
| --- | --- |
| `action/releaseAttestation.js` | The Lit Action: verify HMAC over the raw body, then send `attest()`. |
| `contracts/ReleaseRegistry.sol` | Stores the latest attestation per `(repo, tag)`; only the pinned `attester` (action wallet) can write. |
| `scripts/setup.js` | One-shot: action CID → group → scoped key → derive + fund action wallet → deploy → authorize lit-triggers → create the webhook trigger. |
| `scripts/deploy.js` | Hardhat deploy of `ReleaseRegistry`, pinning the action wallet as `attester`. |
| `scripts/attest.js` | End-to-end client: fire a signed release delivery, then read the registry back. |
| `scripts/_env.js` | Tiny shared `.env` reader / upserter. |

## Walkthrough

### 1. Inputs

```bash
cp .env.example .env
npm install
```

Set in `.env`:
- `LIT_API_KEY` — your **account-level (master)** key from the
  [dashboard](https://dashboard.chipotle.litprotocol.com). Setup mints the
  scoped usage key from it.
- `DEPLOYER_PRIVATE_KEY` — an EOA with **Base Sepolia** gas (any public faucet).
  Deploys the contract and funds the action wallet.

### 2. Run setup

```bash
npm run setup
```

Ten steps: compute the action CID, create a group + scoped usage key, derive
the action wallet, register the action, fund the wallet with gas, deploy
`ReleaseRegistry` (pinning the wallet as `attester`), **authorize this machine
with lit-triggers in your browser** (a page opens — click *Authorize agent*),
and create the webhook trigger. The webhook URL is printed and written to `.env`.

### 3. Fire it

```bash
npm run attest
# or: npm run attest -- --repo owner/name --tag v1.2.3 --commitish main
```

This computes the GitHub-style HMAC over a sample release payload, POSTs it to
the webhook, waits for the run, and reads the registry back:

```
Firing signed release webhook: LIT-Protocol/chipotle v0.0.1-demo @ main
  queued: {"run_id":"…","status":"queued"}
Waiting for the trigger run...
  run status: success
  action result: {"ok":true,"verified":true,"signer":"0x…","repo":"…","tag":"v0.0.1-demo","commitish":"main","txHash":"0x…"}
Reading ReleaseRegistry on-chain...
  getRelease(LIT-Protocol/chipotle, v0.0.1-demo) ->
    commitish: main
    timestamp: 1780015774
✓ Attestation recorded on-chain by the keyless action wallet.
```

### Real GitHub deliveries

Point a repo webhook (Settings → Webhooks) at the printed `WEBHOOK_URL`,
content type `application/json`, secret = `RELEASE_WEBHOOK_SECRET`, events =
*Releases*. Publishing a release (or hitting **Redeliver**) fires the same
path — GitHub does the signing.

## Production considerations

- **Secret handling.** This example stores the webhook secret in the trigger's
  `default_params`. In production, encrypt it with `Lit.Actions.Encrypt` and
  decrypt inside the action so it never sits in plaintext config.
- **Action wallet gas.** Because the action broadcasts, its wallet must stay
  funded. Meter trigger runs and top it up, or move to a "sign authorization,
  caller submits" model if a submitter exists.
- **Registry policy.** `attester` is immutable; rotating the action means
  redeploying or adding a governance-gated `setAttester`. The registry stores
  only the latest attestation per `(repo, tag)`; add history/events indexing if
  you need an audit trail beyond the `Attested` logs.
- **Replay.** GitHub may redeliver; `attest` is idempotent per `(repo, tag)`
  (last write wins). Add a nonce/once-guard if you need exactly-once semantics.
