# Permissionless cross-chain tokens — Hyperlane competitor

**Status:** draft for discussion
**Owner:** chris@litprotocol.com
**Date:** 2026-06-02
**Seeded from:** `examples/cross-chain-token` (`action/bridgeAction.js`, `contracts/BridgeToken.sol`) — promoted into a `lit-bridge` crate and retired as a standalone example (see Repo placement)

**Live (dev, testnet) — confirmed end-to-end 2026-06-02:**
- Registry (Base Sepolia): `0xD1fBb5f03603E82DF13ef94A06b04dC3eA94143D`
- Oracle / signing PKP: `0xe754F8ba7AA0806448430131ca82CE1A1f7CFA7B`  ·  action CID `QmRkM3y3yHH2zzkVJp1BdniJroy7V3mCt8oEurksEgronf`  ·  group `75`
- BridgeToken: Base Sepolia `0xb7F63E48fD5a1d00bC6bAa0202c8b794a7ED18a4` · Arb Sepolia `0xf41e6f64669b01706233c22e90E2ad704d4422DD`
- A 25-token Base→Arb transfer succeeded (manual `bridge.js`): action reached quorum 2 across Alchemy + Infura, signed, minted. (Addresses + the resumable setup live in `lit-bridge/scripts` + the gitignored `.env`.)
- A 10-token Base→Arb transfer succeeded via **relay/auto-broadcast** (`relay.js`): the action verified, signed, and broadcast the mint itself from the oracle account (mint tx status 1, balance confirmed 25→35). Action logic was upgraded (relay mode) with **zero token redeploys** — Option B's payoff: same oracle address, new CID re-pinned.

**Live (Base + Arbitrum MAINNET) — fully autonomous relayer confirmed 2026-06-03:**
- Registry (Base mainnet): `0xCCFB3F7e038688ab7cb4B568D57fE0E97B374326`
- Oracle/signing PKP unchanged: `0xe754F8ba7AA0806448430131ca82CE1A1f7CFA7B` (group `75`); action CID `QmQxsbVWUCqXsnPkCwRog4DzS9MD91vbRmj3bzEcjX14R4`
- BridgeToken (redeployed with fee + gas-prepay logic): Base `0x1C5004e89FC96082D12D4D3Be77268AE16E2af26` · Arbitrum `0xCCFB3F7e038688ab7cb4B568D57fE0E97B374326`; action CID `QmZ24cPQECJCcQRPm7jGAvP76bwxxWGV9iTYLqeZHnFVV4`; triggers Base `1d5af1d3…`, Arb `742c0657…`. (Addresses churn on each redeploy — `lit-bridge/.env` is the live source of truth.)
- **Fee live: 0.1% (10 bps) skim to a treasury (deployer).** Confirmed: a 1000-token Base→Arb relay split 999 to the recipient + 1 to the treasury, autonomously.
- **Native gas prepay live (IGP-style).** `burn()` is payable; the prepay is escrowed in the source token and emitted in `BurnInitiated`. The relayer **only auto-mints when the prepay covers destination gas** (same-native wei comparison) — confirmed live, and this is what makes bridging an **illiquid token safe**: no prepay → relayer skips (no gas drained), holder can self-submit. `sweepGas` (owner) recovers escrow (unit-tested; live sweep pending a deployer Base-gas refill).
- Relayer is the **production `triggers.litprotocol.com`** instance (added `base-sepolia`/`arbitrum-sepolia` to local `CHAIN_SPECS` for dev, but mainnet `base`/`arbitrum` were already supported — so no triggers deploy needed). Two `chain_event` triggers watch `BurnInitiated`.
- **End-to-end autonomous: a burn on Base auto-minted on Arbitrum with no manual step** — poller fired ~30s after the burn; action reached quorum 2, signed, and broadcast the mint (Arb balance 0→5). Registry reads hardened with retry + two keyless Base endpoints (`mainnet.base.org`, `base-rpc.publicnode.com`) after a single-endpoint flake.
- Note: one pre-fix burn (5 BRDG on Base) is burned-but-unminted (orphan from the flaky run; valueless test token, left as-is).

## One-line pitch

Permissionlessly put any token on any chain — same product shape as Hyperlane
Warp Routes, but the verification layer is a content-addressed Lit Action reading
the source chain directly instead of a validator set. **No validators to bootstrap
per route or per chain.** We win on time-to-new-chain, price, and speed; we trade a
validator multisig for "the published action code + the Lit node network + TLS to
named RPCs."

This plan covers two things:
1. The **product gap** vs Hyperlane (what a turnkey "bridge your token" offering needs).
2. The **trust layer** — the crux — built around the idea of moving RPC config
   **on-chain, encrypted, with multi-RPC consensus.** This is what makes "add a new
   chain" a config write instead of a code change, and it's where most of this doc goes.

---

## What Hyperlane's product actually is

"Permissionlessly get a token on a new chain" with Hyperlane = a **Warp Route**, which
is four layers stacked. The protocol is only the bottom one; the rest is what makes it a
*product*:

| Layer | Hyperlane | What we have today |
|---|---|---|
| Transport | Mailbox on every chain | purpose-built per app (no generic bus — fine, we don't need one) |
| **Security** | ISM + off-chain validators sign Merkle roots | ✅ **the action** — CID-signed, finality-checked, RPC-pinned |
| Delivery + economics | Relayers + Interchain Gas Paymaster | ⚠️ Lit Triggers can broadcast; mint is manual today; no fee model |
| Token app | Warp Route routers (collateral / synthetic / native) | ⚠️ `BridgeToken.sol` is a bespoke demo token, not a router for *existing* tokens |
| Tooling | CLI (`warp deploy/send`), Registry, Explorer | ⚠️ `setup.js` is hand-rolled; no registry; no explorer |

We already own the hardest layer (security/verification) and do it with a different,
arguably stronger-for-onboarding trust model. The rest of the product is buildable.

### Product build-list (tracked separately from the trust layer below)
1. **Router standard** — split "the token" from "the bridge": `CollateralRouter`
   (locks an existing ERC-20 on its home chain), `SyntheticRouter` (mint/burn wrapped),
   `NativeRouter`. Lets people bring *their own* token; `BridgeToken.sol` can't today.
2. **N-chain topology** — hub/spoke routing so a burn on any chain mints on any other,
   with replay protection namespaced across all chains.
3. **Automated delivery + economics** — DONE (delivery): the action has a relay mode that
   re-verifies, signs, and **broadcasts the mint itself** from the oracle account; a
   `lit-triggers` `chain_event` trigger fires it on `BurnInitiated`. Verified end-to-end on
   testnet via `scripts/relay.js` (auto-mint, balance confirmed on-chain). lit-triggers reused,
   not rebuilt; added `base-sepolia`/`arbitrum-sepolia` to its `CHAIN_SPECS`.
   **Ops: DONE** — live on the production `triggers.litprotocol.com` poller (Base + Arbitrum
   mainnet); a burn auto-mints on the other chain with no manual step.
   **Economics (option 1 — token skim): IMPLEMENTED + tested.** `BridgeToken.mint` mints
   `feeFlat + amount*feeBps/1e4` to a treasury and the remainder to the recipient (owner-set
   `setFeeConfig`, capped at `MAX_FEE_BPS`=500, fee clamped so the dest mint never reverts on a
   burned transfer). 8 Foundry tests cover the signed-mint path + fee math; `setup.js` applies
   the config (default 0.1% bps). **LIVE on mainnet** — tokens redeployed with fee logic; a
   1000-token relay split 999/1 to recipient/treasury fully autonomously. **Only pending:** the
   treasury→gas conversion loop (deferred — the skim accrues the bridged token, which must be
   sold to refill the oracle's gas).
4. **Interfaces** — the `lit-bridge` **web UI** is the primary surface (connect wallet, pick
   token + chains, bridge, track). An optional CLI/API covers the "deploy a route" path:
   pick token, pick chains, deploy routers, register oracle, wire peers, fund relayer.
5. **Registry + explorer** — `BridgeConfigRegistry` for route/chain discoverability. For v1 the
   UI reads transfer status straight from chain (`BurnInitiated` / `usedBurnIds`); a cached
   indexer for in-flight status, stuck-transfer recovery, and analytics is a *later, separate*
   service (a rebuildable cache over events), added only when performance demands it.

The rest of this document is **layer 2 (security/verification)**, because that's the part
the encrypted-RPC idea changes and the part everything else trusts.

---

## Repo placement: `lit-bridge`, a deployed service (decided)

This ships as a **single productized artifact** — a top-level `lit-bridge/` crate, structured
exactly like `lit-payments` / `lit-triggers` (standalone Rocket service + static web UI +
Postgres migrations + `railway.json` + `Dockerfile` + `contracts/`). **No parallel `examples/`
version.** Most users want to plug into what we run, not rebuild it; maintaining a teaching
example *and* a prod app means building everything twice.

```
lit-bridge/                      # NEW top-level crate, mirrors lit-payments
  src/                           # Rocket: bridging UI host + /api/config (STATELESS — no DB)
  static/                        # bridging web UI (connect wallet, pick chains, track)
  contracts/                     # Foundry: BridgeConfigRegistry.sol + BridgeToken.sol
  action/                        # the bridge verification action (published; CID pinned)
  railway.json + Dockerfile + README.md
  └─ drives → lit-triggers       # relayer: burn event → bridge action → broadcast mint
```

Two consequences that shape the phases:

- **`examples/cross-chain-token` is the seed, then retired.** Its `bridgeAction.js` and
  `BridgeToken.sol` are promoted into `lit-bridge/action` and `lit-bridge/contracts` as the
  starting point; the standalone example directory is removed once `lit-bridge` covers it (a
  code change for Phase 1, not done in this plan edit). We do not maintain both.
- **The relayer is `lit-triggers`, not new code.** "Automated delivery" (watch burn event →
  run the bridge action → broadcast the mint) is exactly a `lit-triggers` chain-event trigger.
  `lit-bridge` drives a `lit-triggers` instance rather than reimplementing event-watching and
  broadcasting — that collapses an entire build phase into configuration.
- **No database — the service is stateless.** All bridge state already lives on-chain: a
  pending transfer is a `BurnInitiated` event, a completed one is `usedBurnIds[burnId] == true`
  + `BridgeMint` on the destination. The UI reads that directly. A fast explorer (cached log
  scans, stuck-transfer detection, analytics) is a *later, separate* indexer over chain events
  — a rebuildable cache, never a dependency of the core service or a source of truth. (We
  deliberately did **not** copy `lit-payments`' Postgres layer; bridge state isn't ours to own.)

---

## The core idea: RPC config on-chain, encrypted, with consensus

### Why today's design hits a wall

`bridgeAction.js` today:
- Pins the RPC **hostname** per chain in a hardcoded `RPC_HOSTS` table (`action/bridgeAction.js:31-55`).
- Takes the full `srcRpcUrl` (**including the Alchemy API key**) as a caller-supplied param,
  validates only its hostname + scheme + chainId + finality, then reads a single RPC.

Two problems for a product:

1. **Adding a chain changes the CID.** `RPC_HOSTS` lives in the action source. Edit it →
   IPFS CID changes → `getLitActionPrivateKey()` signer address changes → **every deployed
   `BridgeToken` must re-point its `bridgeOracle`.** That kills "permissionlessly add a chain."
   The code even says so in a comment at `bridgeAction.js:37-39`.
2. **The API key lives on the caller.** Every caller needs a valid premium RPC key, and it
   travels in plaintext `js_params`. Bad for a permissionless product.

### The fix

Move the per-chain RPC configuration **out of the action source and onto a config-registry
contract on Base**, with the secret (the API key / full URL) **encrypted** so only the
action can read it. Verification reads from **N independent RPCs and requires M-of-N
agreement** on the immutable facts.

This converts "add a chain" from a code change (new CID, mass redeploy) into a **governed
config write** (no CID change, no redeploys). The CID still secures the *logic*; the
registry secures the *RPC list*.

### Architecture

```
  setup (one-time, governed)
  ─────────────────────────
  encrypt RPC URL (host+API key) ──▶ ciphertext     [dark-pool setup.js step 4 pattern]
       (CID-pinned encrypt action + vault PKP)
                                         │
                                         ▼
                         BridgeConfigRegistry  (Base)
                         per srcChainId:
                           expectedChainId, minConfirmations, quorum M
                           tokenAddr, burnTopic
                           rpcs[]:  alchemy|infura -> { type, encApiKey }
                                    custom         -> { type, host, encUrl }
                         owner = Base Safe (the chipotle-upgrades Safe); emits ConfigChanged

  per bridge op
  ─────────────
  burn on src ──▶ caller calls bridge action with {burnTxHash, logIndex, srcChainId}
                                         │
                                         ▼
        ┌──────────────────────────────────────────────────────────┐
        │ bridgeAction (stable CID)                                  │
        │ 1. read config from Base via PINNED bootstrap RPC(s)       │  ◀── only thing still in code
        │    (M-of-N consensus on the config bytes too)              │
        │ 2. Decrypt each rpcs[].encUrl  (TEE-only, CID-gated)       │
        │ 3. verify decrypted host == plaintext host                 │
        │ 4. fetch receipt+block from N providers, require M-of-N    │
        │    agreement on: status, burnBlock, log addr/topics/data,  │
        │    confirmations >= minConfirmations                       │
        │ 5. sign mint authorization (getLitActionPrivateKey)        │
        └──────────────────────────────────────────────────────────┘
                                         │ signature
                                         ▼
                       mint on dest (verifies sig == bridgeOracle)
```

### Provider config model (decision #5)

Each RPC entry is a **tagged union** by provider type. Only the secret is encrypted; the
rest is plaintext on-chain so the action can sanity-check without decryption
(`expectedChainId`, `minConfirmations`, `quorum`, token address, burn-event topic are
per-chain, always plaintext).

- **`alchemy`** — config supplies `{ type, encApiKey }`. The action holds a code-resident
  `chainId → alchemy subdomain` map and **constructs** the URL:
  `https://<subdomain>.g.alchemy.com/v2/<decrypted key>`.
- **`infura`** — config supplies `{ type, encApiKey }`. Code-resident `chainId → infura
  network` map; action constructs `https://<network>.infura.io/v3/<decrypted key>`.
- **`custom`** — config supplies `{ type, host (plaintext), encUrl (ciphertext) }` for chains
  Alchemy/Infura don't cover yet. After decrypting `encUrl`, assert its hostname `==` the
  plaintext `host`, scheme is `https:`, and `eth_chainId == expectedChainId`.

**Why this is the strong version:** for the two default providers the **hostname is
content-addressed in the action code**, not in config — so a compromised config writer can
only supply a key, never redirect an alchemy/infura read at a malicious host. Only `custom`
entries carry a config-controlled host, and those are gated by the plaintext-host equality
check. Encrypt the API key for alchemy/infura; encrypt the full URL for custom.

### Default chains shipped out of the box

The bundled chain set = the **intersection of EVM chains Alchemy and Infura both support**
(Ethereum, Base, Arbitrum, Optimism, Polygon, etc. — authoritative list generated from both
providers' support matrices at build time). Each ships pre-configured with **two** RPCs (one
alchemy, one infura), satisfying the recommended quorum of 2. We supply one Alchemy key and
one Infura key to seed these.

### How encryption/decryption actually works here (important detail)

This repo does **not** use classic `accessControlConditions`. Decryption is gated by a
**CID-pinned group + scoped usage key** (verified pattern in `dark-pool/scripts/setup.js`
and `dark-pool/action/encryptOrder.js`):

- `Lit.Actions.Encrypt({ pkpId, message })` → ciphertext. Anyone can encrypt to a PKP.
- `Lit.Actions.Decrypt({ pkpId, ciphertext })` → plaintext, but **only** when running inside
  an action whose CID is pinned to the PKP's group, under that group's scoped usage key.
- So: encrypt the RPC URL against a **bridge vault PKP** whose group pins the bridge action
  CID. Only that exact action can decrypt. The decrypted key never leaves the TEE (relies on
  Phala TEE confidentiality — consistent with the rest of the system).

Consequence worth internalizing: the ciphertext is bound to the **PKP + group**, not to a
single CID. A group can hold multiple CIDs. That means we can **upgrade the action's logic
and still decrypt the same stored config** by adding the new CID to the group — *without
re-encrypting every RPC entry*. Group membership therefore becomes a trust lever (adding a
malicious CID could decrypt secrets), so **group admin must sit behind the same governance
as the registry.**

### Multi-RPC consensus

For both the **config read** (from Base) and the **burn verification** (from the source
chain), fetch from N independent providers and require **M-of-N agreement**.

- Consensus is on a **canonical hash of the normalized critical fields**, not the raw
  responses (providers differ on irrelevant fields).
- Critical facts for a burn: `receipt.status == 1`, `burnBlock`, the target log's
  `address`/`topics`/`data`, and `confirmations >= minConfirmations`.
- **Do not** require agreement on `eth_blockNumber` (head) — providers sit at slightly
  different heights. Require each agreeing provider to *independently* report
  `head - burnBlock >= minConfirmations`. This is what defangs a single RPC hiding a reorg.
- Liveness: succeed if `M` providers respond and agree, even if others time out
  (`N_available >= M`). Fail **closed** otherwise (better to halt than mint wrongly).
- **Quorum policy (decision #4):** code floor on `quorum` is **1** — a chain *may* run on a
  single RPC (the escape hatch for chains Alchemy/Infura don't cover and the deployer only has
  one URL for). But the **default and recommended quorum is 2**, every bundled default chain
  ships with 2, and the CLI/setup should push hard for ≥2. `quorum == 1` is explicitly the
  reduced-trust mode — it collapses back to today's single-RPC guarantee (TLS + finality only)
  and should be surfaced as such, not silent.
- `minConfirmations` keeps a **floor enforced in code** (CID-secured), so a compromised config
  writer still can't set finality to 0.

Honest caveat: consensus is only as strong as provider **independence**. Several "different"
RPCs that all proxy the same backend (Infura/Alchemy under the hood) defeat the assumption.
The registry should track provider independence; we should *say so* rather than imply N
hostnames == N trust roots.

---

## The bootstrap problem (and its resolution)

To read the config registry on Base, the action needs a Base RPC — which can't come from the
(encrypted) config, because you need a working RPC to read the config in the first place.

**Resolution:** exactly **one** thing stays pinned in the action code — a small set of
**Base config-read RPC hostnames** (keyless public Base RPCs are fine; reading public
contract storage needs no premium key). The action requires **M-of-N consensus across these
pinned Base hosts** on the config bytes. Everything else (all source/destination chains,
their keyed RPCs, finality, quorum) moves to the registry.

Net: code pins one bootstrap surface (how to reach Base config); config provides the rest.
Adding a *bridged* chain = pure config. Only changing the *config-registry chain itself*
(Base) touches code. That's the whole win.

Cost: every bridge op now does one extra chain read (Base), parallelizable, and the bridge
gains a **liveness dependency on Base** — fail-closed, so acceptable.

Alternative considered: pass a **governance-signed config blob** in params (action verifies
the signature over `(config, epoch)`) to avoid the Base read entirely. Avoids the Base
liveness dependency but reintroduces a key/epoch distribution problem. **Decided (#2): read
from Base each op.** Revisit the signed-blob optimization only if Base reads later become a
latency or availability issue.

---

## The signer: Option B, PKP / chain-secured account (decision #1 + #3)

The encrypted-config design makes **adding chains** cheap but does not, by itself, fix
**logic upgrades** — with a CID-derived signer (`getLitActionPrivateKey()`), changing the
action's code rotates the signer and forces re-pointing `bridgeOracle` on every token.

**Decided: Option B.** The action signs mints with a **dedicated PKP / account**
(`getPrivateKey({ pkpId })`), not its CID key. The signer address = the account, **stable
across logic upgrades**. Upgrading the action = add the new CID to the account's group; the
`bridgeOracle` on every `BridgeToken` never changes. Trust for *which code may sign* shifts to
"the group's authorized-CID set is honest" — gated by governance (the Base Safe, #3).

### Custody path: api-key account now → chain-secured account + Safe later

- **During dev:** the signing account is a normal **api-key account** (chipotle). Fast to
  iterate; no on-chain governance wiring yet.
- **Once it works end-to-end:** convert it to a **ChainSecured account owned by the existing
  Base Safe** — the same Safe used for chipotle upgrades — via
  `transferChainSecuredAccountOwnership` (see NODE-4979 / commit `cdc62221`). After conversion,
  the Safe is the sole authority that can change the account's auth and (by extension) govern
  the group's authorized-CID set. This is the single governance root for both the config
  registry (#3) and action upgrades.

The content-addressed guarantee doesn't disappear — it still secures the *logic, finality
floors, and consensus rules* of whatever CID is authorized. We're only decoupling the
*signer identity* from the CID so the product can ship bug fixes without a fleet-wide redeploy.

---

## Honest trust framing (load-bearing)

Be precise about what changes. Today: *"the published action code does what it says, and TLS
to the named RPC isn't compromised."* After this plan:

> The mint signature is honored iff: (a) the **action code** (CID) does what it says — the
> logic, finality floors, provider hostname maps, and consensus rules are content-addressed;
> (b) the **Lit node network** runs it honestly inside the TEE; (c) the **config registry on
> Base** and the **account's authorized-CID set** weren't maliciously changed — both gated by
> the **Base Safe** with public `ConfigChanged` events; (d) **M-of-N independent RPC
> providers** agree on the burn facts (M defaults to 2; M=1 is opt-in reduced trust); and
> (e) TLS to those hosts holds.

What moved: a slice of trust shifts from "code hash" to **"Base Safe governance of the config
registry + the account's CID allowlist."** What we bought: chains are added without redeploys,
the API key never touches callers, action bug fixes ship without re-pointing every token's
oracle, and (at quorum ≥2) a single lying/compromised RPC can no longer forge a mint.

Compared to Hyperlane: they trust a **validator multisig + relayer**; we trust **published
code + node network + governed config + RPC consensus**. Neither strictly dominates — our
pitch is *"no validator set to bootstrap, and you can add a chain in a config tx."*

### Threats and mitigations
| Threat | Mitigation |
|---|---|
| Compromised config writer points oracle at evil RPC | alchemy/infura hostnames are **constructed in code** (CID-secured) — config can't redirect them, only supply a key; `custom` entries gated by plaintext-host equality check; Base Safe governs all writes via public events |
| Config writer sets finality to 0 | `minConfirmations` floor enforced **in code** (CID-secured), not in config |
| Config writer sets quorum to 1 | allowed **by design** (escape hatch for single-RPC chains) but defaults to 2 and is surfaced as reduced trust; bundled chains all ship at 2 |
| Single lying/compromised RPC forges a receipt or hides a reorg | at quorum ≥2, M-of-N consensus on canonical critical-fact hash + per-provider independent confirmation check |
| "Independent" RPCs share one backend | track + surface provider independence; don't overclaim N == N trust roots |
| Base unavailable / config-read fails | fail **closed** (bridge halts; no bad mint) |
| Attacker uploads own ciphertext to registry | registry writes are Safe-gated; decrypt is CID-gated; post-decrypt host check (custom) / code-constructed host (alchemy/infura) |
| Decrypted API key in node memory | TEE (Phala) confidentiality, same posture as the rest of the system |
| Malicious CID added to the signing account's group | account is **chain-secured, owned by the Base Safe**; only the Safe can change auth/CID set; public events |

---

## Phases (check off as you go)

- [x] **Phase 0 — Scaffold `lit-bridge`.** Create the top-level crate from the
      `lit-payments`/`lit-triggers` template (Rocket + static + `railway.json` + `Dockerfile`),
      **stateless — no DB** (bridge state lives on-chain). Promote `examples/cross-chain-token`'s
      `bridgeAction.js` → `lit-bridge/action`
      and `BridgeToken.sol` → `lit-bridge/contracts`; **remove the standalone example** once
      parity is confirmed. All later phases land in `lit-bridge`.
- [x] **Phase 1 — Multi-RPC consensus in the action.** In `lit-bridge/action`, burn facts are
      fetched from N providers and require M-of-N agreement on a canonical critical-fact string;
      per-provider finality check; fail-closed; quorum floor 1, default 2. Pure logic split into
      unit-tested helpers (`node --test`, 15 passing — incl. "a single lying RPC cannot forge at
      quorum 2").
- [x] **Phase 2 — Signer → dedicated account (Option B).** Action signs via
      `getPrivateKey({ pkpId: BRIDGE_PKP_ID })`; `BridgeToken` trusts an immutable oracle address.
      Live: account created, oracle address derived (== PKP address, confirmed by a deriver
      action) and pinned in both tokens.
- [ ] **Phase 2 — Signer → dedicated account (Option B), dev mode.** Switch mint signing from
      `getLitActionPrivateKey()` to `getPrivateKey({ pkpId })` for a dedicated **api-key
      account** (chipotle). Update `BridgeToken.bridgeOracle` to the account address. Authorize
      the bridge action CID on the account's group. Gets the contracts onto the final trust
      model before any registry work.
- [x] **Phase 3 — `BridgeConfigRegistry` + provider-type model + encrypt-and-publish.**
      Contract done (Foundry, Ownable2Step; tagged-union RPC entries; 6 `forge test` passing).
      `scripts/setup.js` runs the full encrypt-and-publish: encrypts the Alchemy + Infura keys
      against the PKP and writes both chains' config (quorum 2). Live and verified.
- [x] **Phase 4 — Action reads config from Base.** Registry-host allowlist + consensus on the
      config read, alchemy/infura URLs built from code-resident maps, `custom` host-equality
      check, `RPC_HOSTS` gone, callers no longer supply keys. Verified end-to-end against the
      deployed registry (a live transfer decrypted both keys in-TEE and reached quorum).
- [~] **Phase 5 — Ship the default chain set.** Demo pair (Base Sepolia ↔ Arb Sepolia) live at
      quorum 2 with a confirmed transfer. **Pending:** generate the full Alchemy ∩ Infura EVM
      chain list and seed each.
- [~] **Phase 6 — Governance handoff to the Base Safe** (`0xF4D0…fD2A`). Fresh **production**
      instance launched on a new dedicated account (new oracle, `Ownable2Step` tokens, registry,
      fee treasury = Safe) — dev instance left untouched. `BridgeToken` made `Ownable2Step` (was
      un-transferable). **Done:** contract ownership of registry + both tokens *proposed* to the
      Safe (handoffToSafe.js, two-step) and fee → Safe (confirmed live). **Pending:** Safe must
      `acceptOwnership()` ×3; and the **irreversible** Lit account chain-secure (convert → 
      `transferChainSecuredAccountOwnership` → Safe, on Base 8453). Current prod addresses live in
      `lit-bridge/.env` + memory. `BridgeToken` trusts the oracle signature alone — no epoch (#6).
      **Account chain-secure DONE** (user converted it; account now Safe-owned). Remaining: the 3
      routine `acceptOwnership()` Safe txs for registry + tokens.
- [~] **Phase 7 — Hardening + adversarial review.** Codex adversarial pass done (7 findings).
      Action-side fixes implemented + unit-tested (21 tests), **no contract redeploy** (destination
      resolved from `bridgePartner` under consensus): registry read quorum≥2 + distinct hosts (the
      critical one — a single lying bootstrap RPC could forge mints), dest-from-`bridgePartner` +
      gas cap == prepay quote, same-native relay guard, distinct-host quorum, dest-RPC chainId
      check. **#4 (finality) partial** — depth is Safe-governed `minConfirmations`; true
      `finalized` finality needs a retry poller (single-fire trigger can't wait) → follow-up.
      **DEPLOYED + validated**: hardened action `QmSt8Z…1miGz` re-pinned via a Safe-signed
      `addActionToGroup` (master apiKeyHash, not the Safe alias — see `docs/upgrading-the-action.md`)
      and confirmed end-to-end (5-token Base→Arb auto-relay: 4.995 net + 0.005 fee to Safe).
      Registry read hardened to 3 reliable keyless endpoints + retries after a quorum-2 flake.

---

## Decisions (locked in 2026-06-02)
1. **Signer: Option B.** Dedicated account (PKP), not CID-derived. Start as an api-key
   account; convert to a **ChainSecured account owned by the existing Base Safe** (the
   chipotle-upgrades Safe) once it works, via `transferChainSecuredAccountOwnership`.
2. **Config delivery: read from Base each op.** Signed-config-blob optimization deferred.
3. **Governance: the Base Safe** owns both the config registry and the signing account.
4. **Quorum: floor 1, default & recommended 2.** All bundled default chains ship at 2.
   `quorum == 1` is the opt-in escape hatch for chains only one provider/URL covers, surfaced
   as reduced trust. `minConfirmations` keeps a hard code floor.
5. **Provider types:** `alchemy` / `infura` (supply api key + chainId; action constructs &
   checks the hostname from code-resident maps) and `custom` (full URL string for chains the
   two don't cover yet; plaintext host + encrypted URL). Default chain set = Alchemy ∩ Infura
   EVM support, seeded with one key each.
6. **Destination trusts the oracle signature alone** — `BridgeToken` does *not* check a config
   epoch.

## Out of scope for v1 (mention as "going further")
- Generic message bus (Mailbox equivalent) — we don't need it for tokens.
- Light-client / ZK verification of source-chain state (consensus-over-RPCs is the v1 stance).
- Non-EVM source chains.
- The full registry/explorer product surface (separate plan; see product build-list above).
</content>
</invoke>
