# Lit Triggers — examples

Working, live-tested examples of Lit Actions driven by Lit Triggers
(webhook / schedule / chain-event). Each folder has an `action.js` (the Lit
Action), a `README.md` with the exact create payload and a sample run, and —
where relevant — the target Solidity contract.

## What makes these different

Every on-chain example signs with the **action's own wallet**, whose private
key is held by the Lit network — no server or human holds it. So an action is
an autonomous actor that reacts to a trigger, evaluates trusted data, and
signs/sends a transaction, with no admin who can forge or rug it. The signing
key is exposed inside the action via `Lit.Actions.getLitActionPrivateKey()`.

Each distinct action's wallet is derived from its code, so every example has a
**different** wallet address — fund the specific address an example reports.

## The Lit Action contract

The runtime wraps your code and invokes `main(params)` itself, then wraps the
returned value in `Lit.Actions.setResponse()`. Do **not** call `main()` yourself.

`params` shape by trigger type:

- **webhook** — `{ source: "webhook", event: <parsed body>, event_raw: <raw string>, headers: { ... } }`
- **schedule** — `{ source: "schedule", scheduled_at, cron }` (flat)
- **chain_event** — `{ source: "chain_event", event: { chain_key, chain_id, decoded: { arg0, arg1, ... }, raw_log, transaction_hash, ... } }`

Available in the sandbox: `ethers` (v5), `fetch`, `crypto`,
`Lit.Actions.{setResponse, getLitActionPrivateKey, getLitActionWalletAddress, Encrypt, Decrypt}`.
`viem` is **not** available.

## The examples

| Folder | Trigger | What it shows |
|--------|---------|---------------|
| [`webhook-echo`](./webhook-echo) | webhook | Starter / smoke test — echoes event + headers |
| [`webhook-notary`](./webhook-notary) | webhook | The minimal "sign with my keyless wallet" primitive |
| [`webhook-release-attestation`](./webhook-release-attestation) | webhook | Verify a GitHub release webhook (HMAC), anchor it on-chain |
| [`schedule-uptime-insurance`](./schedule-uptime-insurance) | schedule | Parametric insurance — autonomous payout when a service is down |
| [`chain-feed-mirror`](./chain-feed-mirror) | chain_event | Relay a Chainlink feed to a chain Chainlink doesn't support |

## dryRun

The on-chain examples accept `dryRun: true` in `default_params`. When set, the
action does everything up to and including signing the transaction, then returns
the signed raw tx **without broadcasting** — so you can verify the logic (and
discover the action's wallet address to fund) before going live. Set
`dryRun: false` once the wallet is funded.

## Configuring secrets

Non-secret config (RPC URLs, contract addresses, thresholds) goes in
`default_params`. The examples also read secrets (e.g. a GitHub webhook secret)
from `default_params` for demo simplicity — in production, store secrets with
`Lit.Actions.Encrypt`/`Decrypt` rather than plaintext `default_params`. The
**signing key is never configured** — it comes from the runtime.

See [`.env.example`](./.env.example) for every input across the set.
