# Venue Layer for Lit Actions + Email Approval Primitive

## Status

Approved direction; implementation started (see `lit-venues/`).

Updated 2026-06-10 per founder direction: **CCXT demoted from required to optional** — we build native per-venue integrations, added as needed (Coinbase support in ccxt is partial in practice, and we'd hand-validate Tier-1 venues anyway). **Egress is buy-not-build**: authenticated proxy/VPN with dedicated static IPs (precedent: we've automated Polymarket from a US server this way). **Legal has signed off** on the TEE-held-credentials model. The file name keeps its original `ccxt-` prefix to preserve links; the design no longer centers on CCXT.

Updated 2026-06-10 (later session): **Hyperliquid promoted from Tier-2 candidate to a Tier-1 launch venue** — the first *PKP-native* venue: there is no API key to seal; the PKP signs EIP-712 directly, by default as a venue-side **agent wallet** that Hyperliquid structurally bars from withdrawing. See D8 and milestone M2.5.

Updated 2026-06-11: **everything code-side of the remaining milestones is built on this PR** — Hyperliquid connector (vectors green, live public conformance PASS), M2 remainder (client pooling, rotation runbook), the full M3 email-approval primitive (both halves, in-TEE verification), the M1/M2.5 conformance gates, the drift canary, and all six M4 examples. What remains is environment, not code: venue keys for the authenticated gates, the new approval env vars on dev, and a staging deploy. See the 2026-06-11 build-out section below.

## Owner

TBD (chipotle core). Counterpart plan: `plans/flows-wallet.md` in [LIT-Protocol/flows](https://github.com/LIT-Protocol/flows) — the two documents share the Integration Contract section below and must be read together.

## Related

- `plans/lit-triggers.md` — trigger service this plan composes with
- `plans/lit-solver-custody-demo.md`, `examples/lit-solver-vault-cowswap/` — on-chain venue precedent (~361ms median warm signature)
- `examples/solana-signer/` — Ed25519 / non-EVM signing precedent
- `TODOS.md` P1 — `max_get_keys_count` enforcement and 403-vs-500 fixes intersect with the quota work in D2

## Why

Chipotle can already sign on any chain from inside the TEE. But the majority of crypto trading volume and nearly all of a trading operation's *operational* risk lives on centralized venues — Binance, Coinbase, OKX, Bybit — behind API keys that today sit in `.env` files, bot platforms' databases, and CI secrets. A venue layer makes every exchange callable from in-TEE JavaScript with credentials that are sealed in Lit and never exposed — to the user's own server, to us, or to anyone.

This produces value nobody else can produce today:

1. **Leak-proof exchange keys.** Binance *requires* IP allowlisting for withdrawal permission, and unrestricted HMAC keys are now read-only by policy. A trade-scoped key bound to a dedicated egress IP that only the user's policy code can reach is unusable anywhere outside that policy. No bot platform (3Commas, Coinrule hold keys server-side), no MPC custodian (Fireblocks/Fordefi govern on-chain transactions, not CEX API keys) offers this. It is the wedge.
2. **One policy governs every venue.** The same Lit Action that signs a CowSwap intent or a Solana transfer can place a Coinbase order or a Hyperliquid perp — on-chain and off-chain execution under a single auditable policy, which is the premise of the Flows Wallet product consuming this layer.
3. **80/20 venue strategy.** We validate the three venues that dominate value production — Binance (deepest global liquidity), Coinbase (deepest regulated US venue), and Hyperliquid (the dominant onchain perp venue, whose agent-wallet model fits a TEE signer perfectly — D8) — to a hard conformance bar, then add connectors strictly on demand. The long tail stays reachable from day one (raw `fetch`, or user-imported CCXT as an experimental path) without us validating 100+ venues we have no user for.

## What ships

1. **`@lit-protocol/lit-venues`** — hand-built, REST-only venue connectors (Binance + Coinbase shipped; Hyperliquid next — D8) behind one unified interface, bundled as a self-contained ESM file (noble crypto inlined, ~tens of KB). Usable **inline** in action code today; published to npm → jsDelivr (integrity-pinned, mirrored) once stable.
2. A venue conformance harness with certification tiers, wired into CI as gates.
3. The `venue-credentials-v1` sealed secret schema (now including optional proxy credentials) and a connect-time verification action.
4. **Proxy-based egress**: authenticated-proxy support in the actions `fetch` op + the dedicated-IP provisioning pattern (IPs purchased from a proxy provider, one per trade-enabled connection).
5. An email send + approval primitive (`sendEmail`, `requestEmailApproval`, `checkEmailApproval`) with in-TEE attestation verification and tiered assurance levels.
6. Six new `examples/` entries that double as certified Flows Wallet templates.

## Design decisions

### D1. Native connectors, with CCXT as an optional long-tail path

**Decision (founder-directed, 2026-06-10):** build thin native clients per venue instead of integrating CCXT wholesale. Rationale:

- **Coinbase first-class.** ccxt's `coinbase` (Advanced Trade) support is partial in practice and has no wired sandbox; we'd be hand-validating and patching around it anyway. Owning the client means owning ES256-JWT auth, error taxonomy, and endpoint coverage outright.
- **Audit surface.** A security product whose pitch is "read the policy code" should not pull a ~53MB-unpacked dependency into the TCB to call two venues. `lit-venues` targets ~1–2k LOC of reviewable TypeScript; the only crypto dependencies are `@noble/hashes` + `@noble/curves` (the same audited primitives ccxt itself uses internally).
- **Runtime fit by construction.** A tiny bundle removes the 64MB-heap concern entirely, and we design for the 50-fetch quota (e.g., `fetchMarket(symbol)` for one symbol's precision rules instead of full `loadMarkets`).
- **Venue churn stays shippable.** Same conclusion as the original red-teamed debate: the connector library is a **CDN module, not a runtime built-in** — a venue API change ships as a library patch in hours, version-pinned per action, never a coordinated TEE network release. Riders carry over: mirrored on infra we control (integrity hashes make the mirror trustless), and a Flows-blessed version channel with managed upgrade prompts.

**Unified interface, ccxt-shaped on purpose** (`fetchTicker`, `fetchMarket`, `fetchBalances`, `createOrder`, `cancelOrder`, `fetchOpenOrders`, `fetchMyTrades`; unified `BASE/QUOTE` symbols) so anyone fluent in ccxt is fluent in lit-venues, and a future ccxt interop or connector swap stays cheap.

**CCXT's place:** users may import it themselves (jsDelivr ESM) for long-tail venues as **Tier 3 / experimental** — unverified on our runtime, their own risk, documented honestly. We spend zero engineering on it unless demand shows up.

**Delivery modes:** (1) **inline** — the bundled ESM concatenated into action code (16MB code budget makes this trivial; this is the v0 path and what the M0 spike uses); (2) **import** — integrity-pinned jsDelivr import once published, with the lit-static mirror as fallback.

### D2. Runtime fit + the (now small) M0 spike

Runtime numbers that shape the library: 16MB code+params, 64MB default memory (per-request `memory_limit` exists if ever needed), 15-minute max execution, **50 outbound HTTP requests per action**, 10 key/signature ops, 1MB response, 100KB logs.

The spike shrinks from "does a 5.5MB library run in Deno" to verifying our own assumptions on the dev environment:

- **In-runtime signing**: `@noble/hashes` HMAC-SHA256 (Binance), `@noble/curves` ed25519 (Binance Ed25519 keys) and P-256 (Coinbase ES256 JWT) — bundled, no Node `crypto`, validated against published test vectors offline first. (`ethers.utils.computeHmac` exists in-runtime as a fallback for HMAC only.)
- **Inline-bundle execution**: an action containing the bundle fetches a public ticker (Coinbase market-data endpoint — geo-safe) end-to-end on dev.
- **Egress geography**: record what dev's CVM egress actually is; api.binance.com and testnet.binance.vision both return HTTP 451 to US IPs (verified), which determines whether M1's Binance gate needs D4's proxy from day one.

Quota discipline stays in the library's design: single-symbol market lookups, no full-market loads, derived-data-only responses (1MB cap), and the chained-trigger pattern (D7) for anything long-running.

### D3. Sealed venue credentials: `venue-credentials-v1`

Venue keys are sealed with the existing PKP-as-vault primitive (`Lit.Actions.Encrypt({pkpId, message})`), group-gated on Base so only actions whose CID is in the connection's group can decrypt. Standard schema so every template interoperates:

```json
{
  "schema": "venue-credentials-v1",
  "venueId": "binance",
  "keyType": "ed25519 | hmac | rsa | es256-jwt | pkp-eip712",
  "apiKey": "...",
  "secret": "...",
  "password": "(optional, e.g. okx)",
  "uid": "(optional)",
  "scopes": ["read", "trade"],
  "egress": {
    "mode": "dedicated | shared",
    "region": "eu-west",
    "proxyUrl": "(optional) http://user:pass@ip:port — sealed with the venue key, one Decrypt gets both"
  }
}
```

Guidance baked into docs and the connect verification action: Binance → Ed25519 keys preferred (Binance's own recommendation); Coinbase Advanced Trade → CDP keys with ES256 JWT (Coinbase explicitly does not support Ed25519 there). One `Decrypt` call per venue per action keeps us comfortably under the 10-key-ops cap. Add a log-scrubbing pass for key-material patterns so a careless `console.log` in user code can't exfiltrate a decrypted secret into the 100KB log channel.

**PKP-native venues** (`keyType: "pkp-eip712"`, currently Hyperliquid — D8) invert the model: there is no venue secret, so `apiKey`/`secret` are absent and the record carries addresses only (`masterAddress`, `agentAddress` — the PKP's eth address — optional `vaultAddress`) plus the optional `egress` block. The record exists for schema uniformity, the connect verification action, and sealed proxy creds where used; decrypting it yields nothing a thief could trade with.

### D4. Egress: dedicated proxy IP per trade-enabled connection — bought, not built

Two physical facts force dedicated IPs. **Per-IP rate limits:** Binance meters request weight per IP, so shared egress means one tenant's aggressive bot starves every other tenant with 429s. **Geography:** binance.com (prod and testnet) rejects US IPs with HTTP 451, so egress region must be selectable per connection.

**Decision (founder-directed): use a commercial proxy/VPN service rather than building an egress fleet.** We have done exactly this before (automating Polymarket from a US server). Shape:

- **Runtime work (the only chipotle build item) — SHIPPED:** a dedicated op `op_lit_proxied_fetch` exposed as `Lit.Actions.proxiedFetch({url, method, headers, body, proxy})`, doing the request in-process via `reqwest::Proxy`. (A per-request `litProxy` init field on the global `fetch` was the original sketch, but Deno's `fetch` drops unknown init fields and its HTTP client is fixed at enclave-init time — so a dedicated op is required, not optional.) Proxy credentials are secret material (never logged; the op is deliberately un-instrumented). Follow-up: pool clients per proxy URL (v0 builds one per call).
- **Provisioning (Flows-side, per the contract):** one **dedicated static IP** purchased from a proxy provider per trade-enabled connection, region-selectable; the IP is known at provision time, recorded on the connection, and rendered in the exchange-side allowlist instructions. Proxy credentials are sealed inside `venue-credentials-v1` (D3) so one decrypt yields key + egress together.
- **Security story intact:** TLS is end-to-end through HTTP CONNECT — the proxy provider sees SNI and timing, never API keys or payloads. The exchange key remains bound to an IP only the user's policy code can egress from, because the proxy credentials themselves are sealed in the TEE.
- Read-only connections may ride the CVM's shared egress directly; dedicated IPs are for trade scope, **included** in the trading tier rather than upsold. *(The red-teamed principle — dedicated IP is the product — is unchanged; only the implementation moved from build to buy.)*

New risk this buys: proxy-provider reliability and ToS. Mitigation: a provider-agnostic proxy abstraction (any `http(s)://user:pass@host:port`), a documented runbook for rotating a connection to a new IP (update exchange allowlist → update sealed creds), and at least two vetted providers before GA.

### D5. Venue certification: tiers + conformance harness

| Tier | Venues (launch) | Bar |
|------|------|-----|
| 1 — Validated | binance (+binanceus), coinbase, hyperliquid (PKP-native — D8) | Native connector; full conformance suite green in CI; on-call owner; documented quirks |
| 2 — On-demand natives | kraken, okx, bybit (candidates) | Promoted by real user demand; each lands with the same conformance suite |
| 3 — Experimental DIY | everything else | Raw `fetch` or user-imported CCXT; importable, unverified, no warranty |

Conformance spec per Tier-1 venue: `fetchMarket`, `fetchBalances`, `createOrder` (limit + market), `cancelOrder`, `fetchOpenOrders`, `fetchMyTrades`, error taxonomy (insufficient funds, bad symbol, rate-limited), all executed *as Lit Actions* against the dev environment. Withdrawal endpoints are explicitly out of scope for v1 (policy-gated sweeps come through the approval primitive, D6). Perp venues extend the suite with the D8 capability set (`fetchPositions`, `setLeverage`, funding); for Hyperliquid's agent mode the withdrawal exclusion is venue-enforced, not merely policy.

How the three Tier-1 venues actually get validated (the honest version):

- **Binance:** spot testnet (`testnet.binance.vision`) via the connector's `sandbox` flag — full order lifecycle. Requires non-US egress (D4 proxy, if dev's egress is US — measured in M0). Fallback while proxies are pending: `binanceus` live read-only.
- **Coinbase:** Advanced Trade has **no real sandbox**, so: (a) live read-only conformance (accounts/products/ticker) in CI with a flagged key; (b) full order lifecycle against **Coinbase Exchange's stateful sandbox** (`api-public.sandbox.exchange.coinbase.com`) to validate auth/order plumbing patterns; (c) a tiny live Advanced Trade order behind a manual CI flag before declaring Tier 1.
- **Hyperliquid:** full order lifecycle on the public testnet (`api.hyperliquid-testnet.xyz`, faucet-funded) as Lit Actions — including the `approveAgent` connect step and a batched order+cancel; signing pinned byte-for-byte to official-SDK test vectors offline first; a tiny live mainnet order behind a manual CI flag before declaring Tier 1 (same bar as Coinbase).

Plus a **daily venue-drift canary** (cron in CI hitting the Tier-1/2 smoke suite) so a Saturday-night exchange API change pages us before it pages users — feeding the status page with per-venue health.

### D6. Email send + approval primitive (answering "where does email go")

The founders' ask: *"someone can code an outbound email to get someone to confirm something in their JS — the human root of trust."* Split decision, adopted after red-teaming a three-service design down to two:

- **Chipotle owns the primitive** (this plan): sending, nonce issuance, attestation signing, and — non-negotiably — **in-TEE verification**. If verification lived in Flows, a Flows compromise could forge approvals and move funds, collapsing the custody story.
- **Flows owns the UX** (flows plan): approval inbox, policy-builder integration, branded pages.
- **lit-triggers is optional glue**, not critical path: an approval-completed webhook can resume an automation.

New `Lit.Actions` ops (gRPC ops to lit-api-server, like sign/encrypt):

- `sendEmail({to, subject, text})` — plain notification. Server-mediated through Resend (already a dependency in lit-triggers) from a fixed network domain (e.g. `actions.litprotocol.com`) with per-account sub-identity, per-account rate quotas, no arbitrary HTML, and links restricted to the approval domain. Server mediation exists because deliverability and abuse control (spam from our domain) cannot be enforced from inside arbitrary user JS.
- `requestEmailApproval({to, summary, assurance, ttlSec})` → `{approvalId}` — issues a single-use nonce, emails a signed approval link hosted by lit-api-server.
- `checkEmailApproval({approvalId})` → `{approved, attestation}` — the attestation is signed by a network attestation key and verified **in-TEE**; it binds `approvalId`, approver email, assurance level achieved, and timestamp.

**Assurance levels** (policies declare what they require — this came out of the red team flipping bare email-link approval for money movement):

- **L1** — link click. For low-stakes confirms ("run the weekly report").
- **L2** — link click + OTP/passkey step-up at the approval page. Email is the *notification* channel, not the *authentication* channel. Required default for anything that moves funds.
- **L3** — EIP-712/EIP-1271 co-sign (EOA or Safe), reusing the existing ChainSecured verification path. For treasury-grade moves.

Actions are request-scoped (15-min cap), so approval is **two-phase by design**: phase 1 requests approval and exits; phase 2 (re-invoked manually, by lit-triggers webhook on approval, or by the next cron tick) checks the attestation and proceeds. The `cex-sweep-with-email-approval` example demonstrates the full pattern.

### D7. Long-running strategies: chained triggers, not long actions

A TWAP or rebalancer is not one 15-minute action; it's a cron trigger + a small action per tick + strategy state persisted between ticks (flow storage or PKP-encrypted state blob). Document this as *the* pattern; the examples implement it. This also keeps each tick within the fetch quota and makes every tick an independently attested, auditable execution.

### D8. Hyperliquid: the first PKP-native venue

**Decision (2026-06-10): promote Hyperliquid from Tier-2 candidate to the third Tier-1 launch venue**, and the first of a new connector class — **PKP-native venues**, where there is no API key to seal because the venue authenticates EIP-712 signatures from an Ethereum key, and that key is the PKP itself.

Why it earns the slot:

- **Demand.** Hyperliquid is the dominant onchain perp venue and the default home of the trading-bot/agent ecosystem — for the Flows Wallet audience it is the most-requested venue after Binance.
- **Strongest possible key story.** CEX mode seals an existing secret; here the trading key *never exists anywhere* — it is MPC-shared PKP material. Connecting as an **agent wallet** (`approveAgent`) makes it stronger still: Hyperliquid structurally bars agents from withdrawals and external transfers, so the venue enforces our no-automated-withdrawals stance for us. Custody stays on the user's master wallet (EOA or Safe); the policy gets trade-only powers, revocable venue-side at any time.
- **Signing is our home turf.** All trading actions are secp256k1 EIP-712 — the exact `signAndCombineEcdsa` path the CowSwap solver vault proved at ~361ms warm. One signature covers a *batched* order action, so the 10-sig-op cap stretches far.

**Two connection modes**, both `venue-credentials-v1` with `keyType: "pkp-eip712"` (D3):

1. **PKP-as-agent (default).** The user's master account signs one `approveAgent` for the PKP's eth address; thereafter the PKP signs orders/cancels only — withdrawals impossible by construction. Agent approvals are revocable and can carry an expiry; the connect flow records both and the drift canary alarms before expiry.
2. **PKP-as-master.** The PKP *is* the Hyperliquid account (funds custodied under it), for fully Lit-native flows; sweeps then go through the D6 approval primitive like any CEX.

Mechanics the connector must own (the correctness-critical bits):

- **Two signing schemes.** Trading ("L1") actions are msgpack-serialized → keccak action-hash → signed as a phantom-agent EIP-712 struct (domain `Exchange`, chainId 1337); user-signed actions (`approveAgent`, transfers) are plain EIP-712 under the `HyperliquidSignTransaction` domain with a real chainId (e.g. 42161). Byte-exact msgpack fidelity is the #1 correctness risk — unit tests pin signatures against the official SDK's vectors before any live call.
- **REST-only fits us:** `POST /info` (public data) and `POST /exchange` (signed actions) — no new transport. A full public testnet with faucet means a **complete order lifecycle in CI**, a better validation story than Coinbase's no-sandbox.
- **Precision model differs from CEX filters:** price ≤ 5 significant figures and at most `MAX_DECIMALS − szDecimals` decimals (6 perps / 8 spot); size quantized to `szDecimals`. Maps cleanly onto the existing decimal-string math; `fetchMarket` derives `priceIncrement`/`amountIncrement` from `meta`.
- **Nonces are ms-timestamps per signer** — fine for our seconds-to-minutes ticks; concurrent same-agent actions are documented as the footgun (one agent per connection avoids it).
- **The unified interface grows an optional perp surface:** `fetchPositions`, `setLeverage`, `fetchFundingRate` (ccxt names, per D1), gated per-venue by the conformance suite; spot-only connectors are untouched.
- **Error taxonomy:** `/exchange` returns per-order status strings — mapped into the same `VenueError` codes (insufficient margin → `insufficient_funds`, unknown asset → `bad_symbol`, throttle → `rate_limited`, L1/API outage → `venue_unavailable`).

**Egress is cheaper here.** Hyperliquid has no exchange-side IP allowlist, and trading throughput is governed by per-*address* budgets (cumulative-volume based) rather than per-IP weight — so D4's dedicated-IP *requirement* doesn't apply and shared CVM egress is acceptable for v1 (per-IP `/info` weights may still argue for spreading read-heavy tenants). Geo posture: the ToS bars US persons (and Ontario) and the app geofences by IP; **measured 2026-06-11 — `api.hyperliquid.xyz` answers a direct US egress (ticker/meta/funding all 200), so no proxy is required for v1**; the k6 gate re-measures from the CVM on every run and `proxiedFetch` (M2, shipped) is the fallback if the venue ever starts IP-gating. Jurisdiction gating in the venue picker is a Flows-side duty, carried on the tier/status feed (contract row 7).

**Monetization note for Flows: builder codes.** Hyperliquid lets an approved builder address attach a per-order fee (the user signs `approveBuilderFee` once, capping the rate). That is a native per-trade revenue channel requiring zero fee infrastructure on our side — the connector simply carries the optional builder field; pricing posture is a flows-plan decision.

## Integration contract with Flows

*(This table is mirrored verbatim in `plans/flows-wallet.md` in the flows repo. Changes require updating both.)*

| # | Artifact | Producer (this repo) | Consumer (flows) |
|---|----------|----------------------|------------------|
| 1 | `@lit-protocol/lit-venues` — native connectors, versioned, integrity-hashed, mirrored; inline-bundle pattern for v0; CCXT optional long-tail | D1 | All wallet templates use it; Flows pins a blessed version and prompts managed upgrades |
| 2 | `venue-credentials-v1` secret schema (incl. sealed proxy creds) + connect verification action | D3 | Venue connect flow seals credentials via PKP vault; Flows stores ciphertext + metadata only |
| 3 | Certified template registry: `examples/` actions published as pinned CIDs in a lit-templates group | D5 / examples | Automations gallery installs by CID; agent invokes only these typed templates |
| 4 | `email-approval-v1` attestation format + assurance levels L1/L2/L3 | D6 | Approvals UX, policy builder, teams maker-checker |
| 5 | Authenticated-proxy `fetch` support + dedicated-IP provisioning pattern | D4 | Flows provisions a dedicated proxy IP per trade-enabled connection, seals proxy creds with venue creds, records the IP, renders allowlist instructions |
| 6 | Execution receipt (attested response envelope) per action run | existing + D5 | Signed, exportable audit log in Flows |
| 7 | Venue tier/status feed (Tier 1/2/3 + drift-canary health) | D5 | Venue picker UI, status page |

## Milestones & gates

All gates run against the **dev environment only** (per our standing deploy guardrails), as k6/e2e specs in CI.

| Milestone | Contents | Gate (must be green) |
|-----------|----------|----------------------|
| **M0 — Spike** ✅ DONE | `lit-venues` scaffold; signing unit-tested against published vectors; inline-bundled action fetches a public ticker on staging; egress-region measurement | `e2e/tests/api/lit-venues-spike.spec.ts` — PASSED on staging (582ms, US egress confirmed) |
| **M1 — Tier-1 conformance** 🟡 awaiting keys | Binance + Coinbase **public** live conformance DONE (real APIs; Binance via proxy); secret schema; markets-cache injection DONE. Gates fully wired and env-gated. **Remaining:** supply Binance-testnet + Coinbase-CDP keys to CI; Exchange-sandbox lifecycle | `k6/correctness/venues-binance.spec.ts`, `venues-coinbase.spec.ts` (built; authenticated tiers skip without keys); `lit-venues/scripts/verify-live.mjs` green |
| **M2 — Proxy egress** 🟡 awaiting deploy | `op_lit_proxied_fetch` + `Lit.Actions.proxiedFetch` built (in-process reqwest, authenticated proxy, 10MiB cap, quota-counted); per-proxy **client pooling DONE**; **rotation runbook DONE** (`docs/runbooks/venue-proxy-rotation.md`). **Remaining:** in-TEE network proof on the next staging deploy | `tests/it.rs::proxied_fetch` (ignored — real network + `LIT_VENUES_TEST_PROXY`); staging e2e post-deploy |
| **M2.5 — Hyperliquid (PKP-native)** 🟡 built | Connector BUILT (`/info` + `/exchange`, perps first); msgpack→EIP-712 signing **pinned to official-SDK vectors, 15 tests green**; `approveAgent` flow; `pkp-eip712` schema; perp surface (`fetchPositions`/`setLeverage`/`fetchFundingRate`); builder codes. **Egress measured 2026-06-11: api.hyperliquid.xyz answers direct US egress — no proxy needed.** **Remaining:** faucet-funded testnet key for the dev lifecycle gate; tiny live mainnet order behind manual flag | `k6/correctness/venues-hyperliquid.spec.ts` (built; lifecycle skips without `HYPERLIQUID_TESTNET_KEY`); `verify-live.mjs` Hyperliquid public 4/4 PASS |
| **M3 — Email approval** 🟡 built | Ops + SDK wrappers + **in-TEE attestation verification (fail-closed)** BUILT; `ApprovalService` BUILT (single-use nonces, L1 link / L2 OTP step-up, k256-signed `email-approval-v1` attestations, per-account quotas, Resend, approval pages). 6 runtime integration tests + 4 service tests green. **Remaining:** dev env config (attestation key pair, Resend key, expose-links), then run the e2e gate | `e2e/tests/api/email-approval.spec.ts` (built): two-phase L2 flow completes; wrong-OTP/replay refused; tampered/expired/misbound attestations rejected in `tests/it.rs` |
| **M4 — Examples + docs + handoff** 🟡 built | **Six examples BUILT**; **daily drift canary BUILT** (`.github/workflows/venue-drift-canary.yml`, auto-files a venue-drift issue). **Remaining:** developer.litprotocol.com docs; status-page wiring; Flows handoff review | Example specs runnable against dev; canary needs repo secrets |

### M0 results (2026-06-10) — gate PASSED

Run against staging (`test.chipotle.litprotocol.com`) via `lit-venues/scripts/spike.mjs`:

- **Bundle runs in the TEE.** `lit-venues` IIFE bundle is **141,840 bytes** (~0.8% of the 16MB code budget; memory concern moot). Unit suite: 23 tests green, signing verified against the Binance docs HMAC vector, RFC 8032 Ed25519, and ES256-JWT verify-roundtrip.
- **Live ticker from inside an action**: Coinbase BTC/USD fetched end-to-end; whole execution (2 venue round-trips) **582ms**.
- **Egress is US-region** (Binance testnet returned 451 from the CVM) → M1's Binance lifecycle gate requires the D4 proxy (or `binanceus` interim), as anticipated.
- Findings: staging enforces billing on management calls (fresh accounts 402 until funded — the M1 CI gate needs a funded account or credit fixture; staging Stripe is test-mode, so the documented top-up flow with a Stripe test card works and is itself now validated). Fixed a latent e2e fixture bug en route: `AddUsageApiKeyRequest.description` is required and the fixture omitted it (422 on deployed envs).

### M1/M2 progress (2026-06-10, session 2) — proxy egress built, live conformance passing

Founder supplied a Webshare proxy key (MX exit IPs) to defeat Binance's geo-block.

- **Binance geo-block defeated, verified.** Direct `api.binance.com` → 451 from our IP; through the MX proxy → 200 with real data. Confirmed at the socket level (egress IP reads as the proxy's) and through the actual connector.
- **Live connector conformance PASS** (`lit-venues/scripts/verify-live.mjs`, real APIs): Binance via proxy — `fetchTicker` (BTC/USDT last≈62236), `fetchMarket` (tick 0.01 / lot 0.00001), bad-symbol→`bad_symbol`; Coinbase direct — `fetchTicker`, `fetchMarket`, bad-symbol→`bad_symbol`. This is the real request-building/signing/parsing/error-mapping path, a tier above the 27 unit tests. Authenticated calls (balances/orders) are wired and skip pending a testnet/CDP key.
- **M2 in-TEE proxy op shipped:** `op_lit_proxied_fetch` (ext/bindings.rs) + `Lit.Actions.proxiedFetch` (02_litActionsSDK.js). In-process `reqwest` (same egress point as Deno's `fetch`, no gRPC, no proto change), per-request authenticated proxy, 10MiB streamed-response cap, fetch-quota-counted via the JS wrapper. **Compiles and executes inside the real runtime** (added integration test `proxied_fetch` in tests/it.rs). Two edition-2024/runtime gotchas resolved and documented in-code: bare `async` in `#[op2(async)]` doesn't parse under edition 2024 → use `#[op2(async(lazy))]`; and `reqwest::Proxy` doesn't forward URL userinfo → credentials are split out and applied via `.basic_auth()`.
- **Known boundary:** the op's *network leg* can't be exercised from the local cargo-test process — reqwest outbound is blocked there (the repo's own real-network test `import_rewrite_cdn` fails identically and is `#[ignore]`d for the same reason). The op is therefore `#[ignore]`d like its sibling; its network path is proven via Node today and runs in CI/staging where reqwest egress works (prod CDN imports prove it). lit-venues' `proxy` option now routes through this op in-TEE via `resolveFetch`/`litActionProxiedFetch` (transports.ts), or an injected proxy-capable `fetchImpl` in Node.

### M2.5–M4 build-out (2026-06-11, session 3)

One sweep landed the remaining code on this PR; what's left is environment, not engineering.

- **Hyperliquid (M2.5):** `lit-venues` 0.2.0 ships the connector — hand-rolled msgpack (insertion-order maps, float-rejecting) + EIP-712, **pinned byte-for-byte to the official SDK's signing vectors** (phantom agent connectionId, GTC/cloid/TPSL orders, vault marker, scheduleCancel, subaccount actions, UsdSend/Withdraw user-signed). Perp surface, agent-mode reads, `approveAgent`, builder codes, 5-sig-fig/szDecimals quantization. 58 unit tests green; live public conformance 4/4 against the real API.
- **M1/M2 remainder:** markets-cache injection across all venues (fetchMarket can cost zero quota); per-proxy reqwest client pooling in `op_lit_proxied_fetch` (warm TLS across calls, bounded pool); `docs/runbooks/venue-proxy-rotation.md`.
- **M3 email approval, both halves.** Runtime: three ops on the existing op stream; `checkEmailApproval` verifies the `email-approval-v1` attestation **inside the runtime** against `LIT_APPROVAL_ATTESTATION_PUBKEY` and fails closed (tampered, expired, misbound, or unsigned ⇒ error, never `approved`). Server: `ApprovalService` (single-use nonced approvals, L1 link / L2 OTP step-up — the OTP returns to the *requesting app*, keeping email as notification channel only; k256-signed attestations; per-account daily quotas; strict text-only templates; Resend with dev-mode fallback) + `/approvals/<id>` pages + `/approvals_pubkey`. **v1 limitation, accepted:** pending approvals are in-memory per instance; a cutover drops *pending* (never approved-and-attested) approvals — shared store is a follow-up.
- **Gates:** `k6/correctness/venues-{binance,coinbase,hyperliquid}.spec.ts` (public always; authenticated tiers env-gated on keys), `e2e/tests/api/email-approval.spec.ts` (two-phase L2, wrong-OTP/replay/deny paths), daily `venue-drift-canary.yml`.
- **M4:** all six examples below, built to the `examples/` conventions and exercised offline against the real bundle.

**Deploy configuration this introduces** (dev first, per guardrails): lit-api-server — `LIT_APPROVAL_ATTESTATION_KEY` (32-byte hex; else an ephemeral key is generated with a loud warning), `RESEND_API_KEY`, `LIT_APPROVAL_PUBLIC_BASE_URL`, `LIT_APPROVAL_MAIL_FROM`, `LIT_EMAIL_DAILY_QUOTA`, and `LIT_APPROVAL_EXPOSE_LINK=true` on dev only. lit-actions runtime — `LIT_APPROVAL_ATTESTATION_PUBKEY` pinned from `GET /approvals_pubkey`. CI secrets for the gates/canary — `LIT_VENUES_PROXY`, `BINANCE_TESTNET_KEY/SECRET`, `COINBASE_KEY_NAME/PRIVATE_KEY`, `HYPERLIQUID_TESTNET_KEY`.

### New examples (each doubles as a certified Flows Wallet template)

1. `venue-portfolio-read` — attested multi-venue balance snapshot (read scope, shared egress).
2. `venue-twap-order` — TWAP via cron-trigger ticks + persisted state (D7 pattern).
3. `price-trigger-stop` — lit-triggers price poller fires a stop/limit order action within policy bounds.
4. `cex-sweep-with-email-approval` — CEX → self-custody sweep gated by an L2 email approval (D6 two-phase pattern).
5. `funding-rate-monitor` — cross-venue funding/basis monitor → `sendEmail` alert (read-only, demonstrates the notification op; Hyperliquid funding becomes the natural second leg once D8 lands).
6. `hl-agent-perp-policy` — a Hyperliquid perp order placed by the PKP as an approved agent wallet, fenced by policy (max leverage, reduce-only outside bounds) — the PKP-native showcase: no credential exists anywhere, custody stays on the user's master wallet (D8).

## Risks & open questions

- **We own venue churn now.** Choosing native connectors over CCXT trades a dependency risk for a maintenance obligation: when Binance changes an endpoint, it's our patch. Accepted deliberately; mitigated by the tiny surface (Tier-1-only), the daily drift canary, and the CDN-module delivery that ships fixes in hours.
- **Coinbase Advanced Trade has no real sandbox** — Tier-1 claim rests on the three-pronged D5 approach; be honest about it in docs.
- **Proxy-provider dependency** — reliability/ToS risk; mitigated per D4 (provider-agnostic abstraction, rotation runbook, two vetted providers before GA).
- **US egress + binance.com 451** — until M2, Binance validation depends on proxy egress or binanceus; measure dev's actual egress location in M0.
- **Email deliverability/abuse** — approval emails landing in spam undermines the human root of trust; warmed domain, DKIM/DMARC, strict content templates, per-account quotas.
- **Quota knobs touch billing guards** — coordinate with the TODOS.md P1 enforcement fixes rather than adding a parallel mechanism.
- **Hyperliquid signing fidelity** — the msgpack action-hash must be byte-exact or orders fail (or, worse, sign something unintended); mitigated by pinning unit tests to official-SDK signature vectors plus the full testnet lifecycle gate (D8/M2.5).
- **Hyperliquid jurisdiction posture** — ToS bars US persons and Ontario; geofencing is IP-based. Flows' venue picker must gate by jurisdiction (feed row 7); CVM egress reachability is measured at the M2.5 gate with the D4 proxy as fallback.
- **Single-operator venue** — Hyperliquid's API and book are one operator's infrastructure on its own L1; volatility-window instability or maintenance is a real availability mode. Same mitigation as CEX churn: drift canary + status feed.
- **Approval store is in-memory (v1)** — pending approvals don't survive a restart/cutover and must complete against the issuing instance; attested approvals are stateless and unaffected. Acceptable for dev/M3 validation; shared store before GA.

*(Resolved: legal review — signed off 2026-06-10. Resolved: CCXT-on-Deno go/no-go — moot; native connectors carry no such risk.)*

## Regulated-brokerage to-dos (forward-looking, per founders)

- **Alpaca**: native adapter for equities (ccxt covers its crypto endpoints only) — post-v1.
- **IBKR**: separate adapter track.
- Signed audit export + reconciliation reports (flows plan D4) as the compliance substrate; SOC2 roadmap; per-jurisdiction venue availability matrix.

## Non-goals (v1)

- WebSocket streaming inside actions (lit-triggers pollers cover it; revisit later).
- Automated withdrawals beyond policy-gated, approval-attested sweeps.
- Building connectors ahead of demand — tiers exist precisely so we don't.
- Hyperliquid vaults/subaccounts, builder-deployed (HIP-3) markets, and HL spot — perps first; each is a cheap follow-up on the same signing path once the Tier-1 gate is green.
- Sub-10ms execution loops. We are an execution engine for seconds-to-minutes strategies, not a quoting engine.

## Design debate record

Per the founders' instruction, every deviation from the original design was argued against a counterargument red-team before adoption. Verdicts affecting this plan, with subsequent founder direction noted:

- **T1** CCXT-as-CDN-module vs. runtime built-in — the debate chose CDN module, decisively. **Superseded in part (2026-06-10):** founders dropped CCXT itself in favor of native connectors; the debate's actual conclusions — library-not-builtin, mirror, blessed-version channel — carry over to `lit-venues` unchanged.
- **T2** email primitive split across three services — modified: verification anchored in-TEE (non-negotiable), Flows owns send-UX, lit-triggers demoted to optional glue.
- **T3** dedicated egress IP as premium — **flipped**: included with every trade-enabled connection; per-IP rate-limit isolation and the leak-proof-key promise make it the product. **Implementation updated (2026-06-10):** purchased dedicated proxy IPs (buy) instead of an in-house egress fleet (build), per founder precedent.
- **P3** positioning — modified to "execution engine for seconds-to-minutes strategies; not a quoting engine."

Full record in the flows plan.
