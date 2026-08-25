# Upgrading the bridge action (Safe-governed)

The lit-bridge signing account is **chain-secured** and owned by the **Base Safe**.
The verification action's IPFS CID is the unit of trust: to ship a new action you
must **re-pin the new CID to the permission group**, and that's an admin write on
the on-chain account-config diamond — which only the Safe can authorize.

This runbook is the result of getting it wrong once (a `GS013` revert). Follow it
and the next upgrade lands the first time.

## Why it's not just "edit and redeploy"

- The relayer + manual callers execute the action under a **scoped usage key**.
  The usage key may only run **CIDs pinned to its group**. Change the action by a
  byte → new CID → it won't run until the new CID is pinned.
- Pinning (`addActionToGroup`) is an **admin write**. The account is chain-secured,
  so the only authorized `msg.sender` is the **Safe** (the account admin).
- The signer (oracle) address does **not** change on an action upgrade (Option B:
  the oracle is the account PKP, not the CID). So **no contract redeploy** is
  needed for an action-only change — just the re-pin + trigger re-registration.

## Key facts / addresses

| Thing | Value |
|---|---|
| Account-config diamond (Base) | `0xaAaAA9120fE271F653cfDb6bf400dB93D2DEa7Aa` (from `GET /core/v1/get_node_chain_config`) |
| Base Safe (account admin) | `0xF4D05744acE4ea008D9A79C9841a621C4a39fD2A` — threshold 1, owner `0x5c011a580164E9ab600A1544c08067440138b865` |
| Proposer (Safe delegate) | `0x484635Ffbb5355f444a07C7F568Ae7D123Fa7862` — key in `.context/.env` `SAFE_PROPOSER_KEY` |
| Group id | `1` |
| Safe Transaction Service | `https://safe-transaction-base.safe.global` |
| `apiKeyHash` (master) | `uint256(keccak256(utf8(LIT_API_KEY)))` — already public on-chain |
| `cidHash` | `uint256(keccak256(utf8(cidString)))` |

## The runbook

1. **Edit + unit-test the action.** `cd lit-bridge/action && node --test`. Keep the
   pure helpers testable; the CID changes on any byte change.
2. **If you changed the `BurnInitiated` event signature**, propagate the new
   signature to `scripts/{burn,relay,bridge}.js` (the topic used for log lookup)
   **and** `scripts/registerTriggers.js` `BURN_EVENT`. Miss one and the poller
   silently never matches (no error, just no relays).
3. **Propose the re-pin** (rebuilds the action, computes the CID, signs a SafeTx
   with the delegate key, POSTs to the Safe Tx Service):
   ```sh
   cd lit-bridge/scripts && node proposeRepin.js
   ```
   It prints the new CID, the `safeTxHash`, and the Safe queue link.
4. **Owner executes.** The delegate can only *propose*; the **owner** (`0x5c01`)
   opens the queued tx in the Safe UI and executes (threshold 1 → one signature).
   - If you re-proposed (e.g. to fix a bad tx), there may be **multiple txs at the
     same nonce** — execute the correct one (its "Simulate" passes) and reject the
     stale one. Executing one invalidates the others at that nonce.
5. **Verify the pin on-chain:**
   ```sh
   # groupIdsForAction(masterHash, newCidHash) should include 1
   ```
   (proposeRepin prints the CID; reuse the verify snippet in this repo's history,
   or call `groupIdsForAction` on the diamond.)
6. **Re-register the triggers** with the new action code (uses the usage key +
   agent token — **no Safe needed**):
   ```sh
   TOKEN=$(cat ~/.lit-triggers/agent-token)
   # delete existing triggers, then:
   TRIGGERS_BASE=https://triggers.litprotocol.com TRIGGERS_AGENT_TOKEN="$TOKEN" node registerTriggers.js
   ```
7. **End-to-end test:** `node burn.js --amount 5` then
   `node watchRuns.js <baseTriggerId>` → confirm the auto-mint.

## Critical gotchas (read before you propose)

- **Use the MASTER `apiKeyHash`, NOT the `keccak256(Safe)` alias.** `addActionToGroup`
  only skips the per-group "manage IPFS IDs" permission check when
  `apiKeyHash == masterHash`. The alias trips that check → custom error
  `0xc5a2be52` → the Safe reverts with **`GS013`** (inner call failed, since
  `safeTxGas`/`gasPrice` are 0). `proposeRepin.js` uses the master hash. The master
  hash is just a hash already on-chain (account key + event topic) — not a secret.
- **The proposer key must be a registered Safe delegate.** Adding it is an
  owner-signed action (Safe UI → Settings → Proposers/Delegates, or the Safe Tx
  Service `/delegates/` endpoint). Without it, proposals 422 with
  `Sender ... is not an owner or delegate`.
- **Simulate with `estimateGas` (from = Safe), not `eth_call`.** `addActionToGroup`
  returns nothing; `eth_call` returns `0x` and hides reverts. `estimateGas`
  surfaces them. (This is exactly how the `GS013` cause was found.)
- **The executor needs gas.** Threshold-1 single-owner Safe → only the owner can
  execute; make sure the owner address is funded on Base. The delegate/proposer
  does not pay gas and need not be funded.
- **The registry read needs ≥3 reliable keyless endpoints (quorum 2).** It's
  M-of-N over the `registryRpcUrls` you pass; with only 2 keyless endpoints, one
  rate-limiting under the burst (getChain + getRpc per chain per provider) drops
  you to 1 vote → `only 1 provider voted; need quorum 2`. Use these (verified
  8/8 under burst): `base-rpc.publicnode.com`, `1rpc.io/base`,
  `gateway.tenderly.co/public/base`. Avoid `mainnet.base.org` (flaky), `drpc.org`,
  `meowrpc`, `blockpi` (rate-limit/fail). The action retries each call 5× with
  backoff, but reliable endpoints + a 3rd for slack is what makes it solid.
- **Re-pin is additive.** The old CID stays pinned until you
  `removeActionFromGroup` it. Harmless (the triggers run the new code), but you can
  clean up old CIDs.
- **Don't pipe `setup.js` through `head`** — SIGPIPE kills it mid-run. It's
  resumable (writes to `.env` per step), so just re-run if interrupted.

## Same flow for the registry / tokens

`BridgeConfigRegistry` and the `BridgeToken`s are `Ownable2Step` owned by the same
Safe. Owner-only writes there (`setChain`, `setFeeConfig`, `setBridgePartner`,
`sweepGas`, `transferOwnership`) are ordinary Safe transactions — build the
calldata and propose it the same way (`proposeRepin.js` is a template: swap the
`to`/calldata). These are *contract* calls (not the Lit diamond), so no
`apiKeyHash` subtlety — just the normal Safe propose → owner execute.

**The handoff is two-step + per-chain.** `handoffToSafe.js` does
`transferOwnership(Safe)` (sets `pendingOwner`); the Safe then calls
`acceptOwnership()` — propose those with `proposeAccepts.js`. CRITICAL: **a Safe
exists per chain.** If you `transferOwnership` a contract on chain X to a Safe
address that was only deployed on chain Y, nothing can accept it on X (no code at
that address). Before handing off a contract, deploy the same Safe address on its
chain (Safe{Wallet} → Add network) and register the proposer as a delegate on
that chain's Safe Transaction Service. `proposeAccepts.js` checks for the Safe per
chain and skips (with a warning) any chain where it's missing.
