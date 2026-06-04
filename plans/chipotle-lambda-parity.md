# Chipotle: Lambda-Parity for Frontend-Callable Actions

Status: design proposal / work plan
Author: Chris (with Claude)
Date: 2026-06-04
Related: [private-apps-backend.md](./private-apps-backend.md) — this is the
gateway-side dependency that lets that plan drop its relay requirement.

## Goal

Make a Chipotle **usage API key safe to embed directly in a frontend**, with
abuse/spend blast-radius control on par with a **public AWS Lambda Function URL**.
Achieving this lets "private apps" host only a frontend and call Lit Actions
directly — no relay needed.

## The framing (why this is achievable)

A public Lambda URL is itself a credential-free, internet-callable endpoint.
Anyone can hit it; AWS's protection is **not secrecy**, it's **bounded,
configurable blast radius**: the URL invokes only that one function, throttled,
concurrency-capped, with budgets + alarms. A determined griefer can still run up
*capped* cost — parity means the worst case is a number you chose and got alerted
about, not "drains the account."

Two things are notably **out of scope of the risk** and must stay that way:

- **Confidentiality is unaffected.** Action plaintext/secrets are TEE- and
  CID-gated. Griefing a public key burns money/availability; it can never read
  encrypted data. Nothing here touches that guarantee.
- **CID scoping already bounds *what* runs.** Usage keys are group-scoped
  (`execute_in_groups`), groups pin exact CIDs, and execute permission is checked
  on-chain (`accounts/mod.rs::can_execute_action`). A leaked key on a group that
  pins only your public actions can run **only those actions** — nothing else,
  no account management. This is the "Function URL → one function" property, and
  it already exists.

So the gaps are all about bounding **rate, concurrency, and spend per key**, plus
defense-in-depth.

## What already exists (verify + document, don't rebuild)

| Capability | Where | Status |
|---|---|---|
| Per-key CID scoping (leak radius = pinned actions) | `accounts/mod.rs::can_execute_action`; group `cid_hashes` | ✅ works |
| Global overload shedding → 429 | `core/v1/guards/cpu_overload.rs` (CPL-202, commit d2743fdb) | ✅ but global, not per-key |
| Account-wide prepaid credits → 402 when empty | `core/v1/guards/billing.rs::BilledLitActionApiKey`; `stripe.rs` | ✅ but account-wide, crude |
| Per-second execution charge | `actions/client/execution.rs::flush_unbilled_seconds`; `stripe.rs` `COST_LIT_ACTION_PER_SECOND_CENTS` | ✅ |

## Architecture: where state lives & the zero-latency path

The hot path today is already built on one principle, and everything here follows
it: **authoritative data lives in a slow store (chain or Stripe); the request path
only ever touches an in-process [moka](https://docs.rs/moka) cache with TTL +
stale-while-revalidate (SWR) + request coalescing; staleness is tolerated within
the TTL; money/usage settles asynchronously off the response path.** We are not
inventing an architecture — we are adding cached fields that obey the existing one.

What the code already establishes (verified):

- **On-chain permission reads are already cached on the hot path.**
  `accounts/blockchain_cache.rs` caches `canExecuteAction` / `canUseWalletInAction`
  / wallet derivation (60-min TTL, per-account **generation-counter** invalidation
  bumped on any write). The key's on-chain record is in memory when a request runs.
- **Stripe reads are cached too.** `stripe.rs`: `wallet_cache` (key→wallet, 1h),
  `customer_cache` (10m), `balance_cache` (10m TTL + **SWR** + background refresh +
  coalescing). The credit check in `BilledLitActionApiKey::from_request` is a
  memory read after warmup.
- **Charging is off the response path.** `stripe::charge()` reads the cached
  balance, does an **optimistic in-memory decrement**, records the event, and
  `spawn`s the real Stripe balance transaction fire-and-forget (there's a
  `billing.charge.settlement_failed` metric for when it doesn't land).
  `flush_unbilled_seconds` awaits only the cache ops, not the network. So
  per-request charging costs microseconds, not a round trip.

### The zero-latency gate

The on-chain key record is **already read and cached** on the hot path (it's what
`canExecuteAction` returns). So we pack a single **`hasSpendingRules` bit** into
that record. Reading it costs **zero** extra network and zero extra cache lookup —
it rides along in data already in memory. Fetched in the **same multicall** as
`canExecuteAction` (trivial Solidity change — return multiple values), even a cold
cache miss adds no extra round trip.

```
guard (before execution):
  (canExec, hasSpendingRules) = blockchain_cache.permissions(keyHash)  # 1 cached multicall
  if !hasSpendingRules:  return Success         # ← the 99% with no caps: ZERO added latency
  else:                  enforce rules (all in-memory; see below)
```

### Where each kind of state lives

The three things we store have different shapes (config vs. counter, write-rarely
vs. write-per-request), so they don't share one home:

| Data | Shape | Home | Hot-path read |
|---|---|---|---|
| `hasSpendingRules` flag | 1 bit, toggled rarely | **On-chain** (opt 2), in the key record | Free — already in `BlockchainCache` |
| Cap + window, rate/burst, concurrency, IP/origin allowlist | Config, edited occasionally | **lit-payments DB** (opt 3), edited via its backend+frontend | New moka cache (TTL+SWR), **only read when flag set** |
| Per-key cumulative spend (rolling window) | High-write counter, durable | **lit-payments DB**, written on the existing **async** charge path | Optimistic in-memory decrement (mirrors `balance_cache`) |
| Rate-limit token buckets, concurrency counts, per-IP counters | High-write, ephemeral, rolling | **In-process memory** (per-node) | Native — already in memory |

**Why this split:**

- **Flag on-chain, not the cap value.** The flag is a permission (belongs with the
  others) and is the one thing that must be free to read for *everyone*. Toggled
  only when a key gains/loses its first rule (1 tx, rare). Keeping the cap *value*
  off-chain means tuning caps in the lit-payments UI needs no gas/tx, and the DB
  read is gated behind the flag so non-cap accounts never touch it.
- **Rule details + per-key usage in the DB, not Stripe metadata.** Stripe metadata
  is per-*customer* (account), not per-key; writing it is an API call; it's the
  wrong tool for rolling windows, rate config, or analytics. Keep Stripe for the
  money relationship that already exists (opt 1 stays as-is).
- **Counters in memory, per-node.** Rate buckets / concurrency counts can't go
  on-chain (per-request writes) or in Stripe. Per-node is acceptable because the
  **durable spend cap is the real backstop**; per-node rate limiting yields an
  effective cluster rate of ≈ limit × N nodes, which is fine for griefing control
  (AWS API Gateway throttling is approximate too). Start per-node; add a shared
  store (Redis-like) only if cluster-exact limits are ever required.

### Request flow for an opted-in key

```
1. blockchain_cache: (canExecuteAction, hasSpendingRules)   1 cached multicall
2. flag set → rules_cache.get(keyHash)        memory; DB read only on cold miss (SWR)
3. spend_cache.get(keyHash) vs rolling cap    memory; reject 402 if over
4. rate bucket + concurrency counter          memory; reject 429/503 if over
5. origin / IP allowlist check                memory; reject 403 if not allowed
6. execute
7. POST-response, in the existing spawned settlement task:
     - Stripe balance txn (already there)
     - increment per-key rolling spend in DB + optimistic in-mem decrement
```

Steps 1–5 are memory reads (low microseconds). Step 7 is entirely off the response
path. An opted-in key pays one cold DB read per ~TTL window; a no-cap key pays
nothing. The staleness tolerance is the same bargain CPL-246 already accepted for
balances: a tiny possible overspend inside the TTL window in exchange for a fast
hot path.

### Resolved design decisions

1. **Cap semantics → rolling window** (match AWS Budgets / Lambda), not a lifetime
   balance. The per-key spend counter resets on the window boundary.
2. **Rules-cache freshness → SWR** (short TTL + background refresh, no cross-service
   invalidation plumbing), mirroring `balance_cache`. A cap edit takes effect
   within the TTL window.
3. **Counter durability → per-key spend reloads from the DB on cache miss**
   (durable across deploys/restarts); rate/concurrency buckets may reset on restart
   (acceptable — the spend cap is the durable backstop).
4. **`hasSpendingRules` is fetched in the same contract multicall as
   `canExecuteAction`** so a cold cache miss adds no extra round trip (a simple
   Solidity change to return multiple values).
5. **"Turn rules on" ordering:** write the DB rule first, *then* flip the on-chain
   bit. The bit going true is what activates enforcement, so this ordering avoids a
   window where the flag is set but rules aren't loaded.

## Work items, in priority order

Priority = how much it bounds the worst case per dollar of effort. P0 items are
the ones that turn "drains the account" into "burns a number you chose."

---

### P0.0 — `hasSpendingRules` flag + multicall  (foundation for everything below)

**What.** A single `hasSpendingRules` bit on the on-chain key record, returned in
the **same multicall** as `canExecuteAction`, surfaced through `BlockchainCache`,
and a request guard that short-circuits to "no enforcement" when it's false.

**Why first.** This is the zero-latency gate. Every per-key control below
(spend cap, rate, concurrency, origin) reads it to decide whether to do *any* extra
work. Land it first so the rest can assume "if we got here, rules exist." It also
guarantees the headline property: **keys with no rules pay zero added latency.**

**Build.**
- Solidity: extend the `canExecuteAction` read to also return `hasSpendingRules`
  (decision 4 — just more return values).
- `accounts/blockchain_cache.rs`: cache the bit alongside the existing
  `execute_action` / `execute_and_wallet` results (same generation-counter
  invalidation, so flipping it bumps the generation and is picked up immediately).
- A request guard (sibling to `BilledLitActionApiKey` / `cpu_overload`) that reads
  the cached bit and returns `Success` immediately when false.

**Acceptance.** A key with no rules executes with no extra reads beyond today's
cached permission check; flipping the bit on-chain takes effect on the next request
(generation bump), no redeploy.

---

### P0.1 — Per-key spend cap with auto-disable  ⭐ highest leverage

**What.** A usage key carries its own budget (e.g. credits or a cents cap over a
window). Each execution decrements it; when exhausted the key is rejected (and
flagged disabled), independent of the account's overall balance.

**Why it's #1.** This is the single control that converts the failure mode from
"a leaked frontend key can spend the whole account" into "a leaked key can spend
*its* budget, then stops." It's the analog of an AWS per-function/per-budget cap.

See [Architecture: where state lives](#architecture-where-state-lives--the-zero-latency-path)
for the storage split this builds on. The cap is a **rolling window** (decision 1):
a per-key spend-to-date counter that resets on the window boundary, checked against
a cap configured in the lit-payments DB, gated by the on-chain `hasSpendingRules`
flag (P0.0) so non-cap keys pay zero latency.

**Build.**
- **On-chain:** the `hasSpendingRules` bit lands via P0.0 (fetched in the
  `canExecuteAction` multicall). The existing no-op `balance` field on the key
  (`add_usage_api_key` in `core/v1/models/request.rs`; hardcoded `10_000_000` in
  `account_management.rs`, never read) can be repurposed as the flag or retired.
- **DB (lit-payments):** store the cap + window per key; track the per-key rolling
  spend-to-date. New endpoints on the lit-payments backend to read/set them.
- **Hot path (`guards/billing.rs`):** when the flag is set, read the cached rules +
  cached per-key spend (both in-memory; cold miss = one DB read, SWR) and reject
  with **402** if the rolling spend would exceed the cap. Mark the key disabled for
  the remainder of the window so subsequent calls short-circuit.
- **Async (off response path):** in the existing spawned settlement task that
  already writes Stripe (`stripe::charge`), also increment the per-key rolling
  spend in the DB and apply the optimistic in-memory decrement (mirror
  `balance_cache`). Per-key spend reloads from the DB on cache miss (decision 3).

**Acceptance.** A key with a $X/window cap, hammered in a loop, stops at ~$X spent
within the window and is rejected until the window rolls; the account's other keys
and overall balance are untouched; a key with no rules sees no added latency.

---

### P0.2 — Per-key + per-IP rate limiting (rate + burst)

**What.** Token-bucket throttle keyed on the API key, and a coarser one keyed on
client IP, on the `/core/v1/lit_action` path. Configurable rate + burst per key.

**Why.** Today the only throttle is the **global** CPU-overload 429 — it protects
the node, not your wallet, and one abusive key can still spend fast within global
capacity. This is API Gateway's per-key/per-stage throttling.

**Build.**
- A request guard (sits with the other `core/v1/guards/`) that runs before
  execution, after key resolution. Reuse the existing 429 response plumbing from
  CPL-202 so OpenAPI/docs stay consistent.
- Limits per key default to sane values, overridable per key (store next to
  `balance`).
- Per-IP limit as a second bucket (defense against many anonymous callers on one
  public key).

**Acceptance.** A key exceeding its configured rate gets 429 with a `Retry-After`;
other keys unaffected; the global CPU guard still independently sheds load.

**Decided.** Bucket store is **in-process, per-node** (decision in Architecture):
effective cluster rate ≈ limit × N nodes, acceptable for griefing control since
the spend cap (P0.1) is the durable backstop. Add a shared store (Redis-like) only
if cluster-exact limits are ever required. Buckets gated behind `hasSpendingRules`
(P0.0) so non-rule keys are untouched.

---

### P1 — Per-key concurrency cap (reserved-concurrency analog)

**What.** Cap simultaneous in-flight executions per key.

**Why.** Rate limits bound requests/sec; a concurrency cap bounds *simultaneous*
spend and node load — the thing AWS reserved concurrency exists for. Cheap ins-
urance once P0.2 exists; together they tightly bound burn velocity.

**Build.** A counter (incremented at execution start, decremented at end/timeout)
in the execution dispatch path (`actions/client/execution.rs` / the dispatch in
`lit-actions/server`), checked against the key's configured max. Over-cap →
429/503.

**Acceptance.** A key with concurrency=N never has >N actions running at once;
the N+1th waits or is rejected per policy.

---

### P2.1 — Per-key origin allowlist (replace wildcard CORS)

**What.** Each key (esp. public ones) carries an allowed-origins list; the gateway
sets/validates CORS against it instead of today's wildcard.

**Why.** Today CORS is `AllowedOrigins::all()` (`main.rs`). An origin allowlist is
standard hygiene for a browser-callable endpoint and stops casual cross-site
reuse. **Defense-in-depth only** — `Origin`/`Referer` are browser-enforced and
trivially spoofed by non-browser clients, so it rides *on top of* P0.1/P0.2,
never instead of them.

**Build.** Add `allowed_origins` to the key model; per-request CORS reflection +
rejection in the Rocket CORS layer / a guard. Empty list = same-as-today (or
deny, for `public` keys — see P2.2).

**Acceptance.** A public key configured for `app.example.com` rejects browser
calls from other origins; server-to-server calls still work (documented as not a
security boundary).

---

### P2.2 — "Public / frontend-safe" key type + Dashboard guardrails

**What.** A flag marking a key as intended for the browser. The Dashboard (and
API) then *require* the blast-radius controls to be set before issuing it: a spend
cap (P0.1), rate limits (P0.2), concurrency cap (P1), and an origin allowlist
(P2.1).

**Why.** Makes the safe path the default path. Without this, someone ships an
unbounded key to the browser and we're back to square one. This is the DX glue
that makes the whole effort land.

**Build.** A `public: bool` (or key class) on the key model; validation that
public keys have non-default caps; Dashboard UX that surfaces "this key is
frontend-safe; here's your max blast radius: $X/day, N rps, M concurrent."

**Acceptance.** Creating a public key without caps is refused with a clear message;
the Dashboard shows the bounded worst case for a public key at a glance.

---

### P3 — Optional hardening (post-parity)

- **Alerting / budget alarms.** Spend-velocity alerts and a notification when a
  key auto-disables (the CloudWatch-alarm analog). Parity = bounded worst case
  **+ you find out**.
- **App-check / PoW / captcha** for *unauthenticated* public endpoints, to raise
  the cost of scripted abuse before it hits the spend cap.
- **Per-IP WAF-style rules** (geo, known-bad ranges, anomaly) beyond the P0.2
  bucket.
- **Gateway authorizer hook** for teams that want auth enforced at the edge in
  addition to in-action.

## Suggested sequencing

```
P0.0  hasSpendingRules flag + multicall  ← foundation; the zero-latency gate
P0.1  per-key spend cap (rolling) + auto-disable   ← biggest blast-radius cut
P0.2  per-key + per-IP rate/burst        ← parallel-able with P0.1, both gated by P0.0
  └─ P1   per-key concurrency cap         ← small add once P0.2 lands
P2.1  per-key origin allowlist           ← independent; defense-in-depth
P2.2  public key type + Dashboard caps   ← depends on P0.0/P0.1/P0.2/P1/P2.1 existing
P3    alarms / app-check / WAF / authz   ← post-parity hardening
```

After **P0.0 + P0.1 + P0.2 + P1 + P2.2**, a usage key is safe to ship in a frontend with
bounded, configurable blast radius — Lambda parity — and
[private-apps-backend.md](./private-apps-backend.md) can make its relay optional.

## The honest caveat (same one AWS has)

None of this makes a public endpoint un-griefable. A determined attacker can run
up cost *within the caps you set*. Parity is **bounded, configurable worst case +
alerting**, not zero risk. The caps (P0/P1) make the worst case a number you chose;
the alarms (P3) make sure you hear about it. That is exactly what AWS reserved
concurrency + Budgets + CloudWatch deliver — and what this plan brings to Chipotle.
