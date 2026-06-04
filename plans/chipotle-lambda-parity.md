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

## Work items, in priority order

Priority = how much it bounds the worst case per dollar of effort. P0 items are
the ones that turn "drains the account" into "burns a number you chose."

---

### P0.1 — Per-key spend cap with auto-disable  ⭐ highest leverage

**What.** A usage key carries its own budget (e.g. credits or a cents cap over a
window). Each execution decrements it; when exhausted the key is rejected (and
flagged disabled), independent of the account's overall balance.

**Why it's #1.** This is the single control that converts the failure mode from
"a leaked frontend key can spend the whole account" into "a leaked key can spend
*its* budget, then stops." It's the analog of an AWS per-function/per-budget cap.

**The shortcut.** The usage-key struct **already has a `balance` field** stored
on-chain (`add_usage_api_key` in `core/v1/models/request.rs`; written in
`accounts/mod.rs::setUsageApiKey`, currently hardcoded to `10_000_000` in
`account_management.rs` and **never read** — a no-op today). Most of the schema
exists; the work is enforcement, not new data modeling.

**Build.**
- Read the per-key `balance` in `guards/billing.rs::BilledLitActionApiKey::from_request`
  alongside (or instead of) the account customer balance.
- Decrement it in the post-execution charge path
  (`actions/client/execution.rs::flush_unbilled_seconds`).
- On exhaustion: return 402 *for that key* and mark it disabled so subsequent
  calls short-circuit before execution.
- Expose `balance` / `daily_cap` as real fields in the create-key API + Dashboard;
  let it be account-funded (sub-allocate from the account's credits) or topped up.

**Acceptance.** A key with a $X cap, hammered in a loop, stops at ~$X spent and is
flagged disabled; the account's other keys and balance are untouched.

**Open Qs.** Cap semantics — lifetime balance vs. rolling daily/monthly window
(AWS Budgets is windowed; lifetime is simpler). On-chain write cost of decrement
(may need an off-chain ledger with periodic on-chain settlement, like the
existing Stripe optimistic-decrement pattern).

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

**Open Qs.** Bucket store — in-process (per node, simplest, looser across a
cluster) vs. shared (Redis-like, accurate but new dependency). Per-node is likely
fine given the spend cap (P0.1) is the real backstop.

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
P0.1  per-key spend cap + auto-disable   ← do first; biggest blast-radius cut,
                                            schema field already exists
P0.2  per-key + per-IP rate/burst        ← parallel-able with P0.1
  └─ P1   per-key concurrency cap         ← small add once P0.2 lands
P2.1  per-key origin allowlist           ← independent; defense-in-depth
P2.2  public key type + Dashboard caps   ← depends on P0.1/P0.2/P1/P2.1 existing
P3    alarms / app-check / WAF / authz   ← post-parity hardening
```

After **P0.1 + P0.2 + P1 + P2.2**, a usage key is safe to ship in a frontend with
bounded, configurable blast radius — Lambda parity — and
[private-apps-backend.md](./private-apps-backend.md) can make its relay optional.

## The honest caveat (same one AWS has)

None of this makes a public endpoint un-griefable. A determined attacker can run
up cost *within the caps you set*. Parity is **bounded, configurable worst case +
alerting**, not zero risk. The caps (P0/P1) make the worst case a number you chose;
the alarms (P3) make sure you hear about it. That is exactly what AWS reserved
concurrency + Budgets + CloudWatch deliver — and what this plan brings to Chipotle.
