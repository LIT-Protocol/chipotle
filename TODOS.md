# TODOS

> Status verified against the code on 2026-06-25. Items confirmed implemented were
> moved to **Completed** with evidence pointers. Items still open are listed below.

## Dashboard: setOnAuthReady fires before init* complete
- **What:** Move `setOnAuthReady(...)` call inside `init()` after all `init*` calls, instead of at module evaluation time in `app.js`.
- **Why:** When a user is already logged in from a previous session, `_onAuthReady` fires during `initLogin`, before `initWallets/initGroups/initActions/initKeys` have attached their button listeners. Load functions work fine but button-disable logic hasn't run yet, allowing brief duplicate-click window.
- **Effort:** XS (CC: ~2 min)
- **Priority:** P2
- **Depends on:** None
- **Context:** Introduced by module refactor (PR1). Low risk but a real ordering hazard.
- **Status (2026-06-25):** NOT DONE. `setOnAuthReady(...)` is still registered at module-eval time (`app.js:279`), before `init()` runs at `app.js:327`.

## Dashboard: _confirmResolve promise race in ui-utils.js
- **What:** Guard `confirmDelete()` to reject or queue if a confirm dialog is already pending. Currently a second concurrent call overwrites `_confirmResolve`, permanently leaking the first promise.
- **Why:** Any `await confirmDelete(...)` caller that loses the race hangs silently forever. The `showActionProgress` non-dismissible modal reduces the window but doesn't eliminate it.
- **Effort:** S (CC: ~5 min)
- **Priority:** P2
- **Depends on:** None
- **Context:** Pre-existing in the monolith. Found by adversarial review during PR1 ship.
- **Status (2026-06-25):** NOT DONE. No concurrent-call guard at `ui-utils.js:150-168`; `_confirmResolve` is still overwritten unconditionally.

## Dashboard: msOutside listener accumulation in groups.js
- **What:** Remove the `document` click listener (`msOutside`) when the multi-select modal closes, instead of relying on the `!wrap.isConnected` guard on next click.
- **Why:** Opening and closing modals 10+ times without a subsequent document click accumulates orphan listeners on `document`. Memory/performance leak in long sessions.
- **Effort:** S (CC: ~5 min)
- **Priority:** P3
- **Depends on:** None
- **Context:** Pre-existing in the monolith. Found by adversarial review during PR1 ship.
- **Status (2026-06-25):** NOT DONE. `groups.js:79` still relies on the `!wrap.isConnected` self-removal guard on the next click; no explicit `removeEventListener` on close.

## Dashboard: Form values read after closeModal in keys.js
- **What:** In `openUsageKeyModal` save handler, collect all form values (group IDs, permission checkboxes) *before* calling `closeModal()`.
- **Why:** Currently fragile — `closeModal` only hides the overlay but doesn't clear DOM. If `closeModal` is ever changed to clear innerHTML, `getSelectedGroupIds` would return empty arrays, silently creating a key with zero permissions.
- **Effort:** XS (CC: ~2 min)
- **Priority:** P3
- **Depends on:** None
- **Context:** Pre-existing in the monolith. Found by adversarial review during PR1 ship.
- **Status (2026-06-25):** NOT DONE. `keys.js` reads name/desc before `closeModal()` (line 212) but `getSelectedGroupIds(...)` is still called *after* close (line 215).

## P2: Startup Stripe key validation

**What:** Add `stripe::validate_key()` that calls `GET /v1/balance` at startup. Auth errors (401) are fatal (exit). Availability errors (5xx, timeout) are graceful (billing disabled, retry on first billing request).

**Why:** Currently `stripe::init()` only checks if env vars are present and non-empty. If the keys are invalid (revoked, wrong environment), billing silently fails on the first user request instead of at startup.

**Pros:** Immediate feedback on bad keys. Prevents silent billing failures. Catches test keys accidentally used in production (Stripe returns different key prefixes for test vs live).

**Cons:** Adds a network call at startup (~200ms). Requires the Stripe client refactor (preserve HTTP status, add timeouts) which shipped in PR #184.

**Context:** Identified during CEO plan review (2026-03-26). Codex flagged that the original Stripe client (`stripe_get`/`stripe_post` helpers) throws away HTTP status codes and uses a no-timeout reqwest::Client, making auth/availability distinction unreliable. The client refactor (PR #184) fixes that prerequisite.

**Effort:** S (human: ~2h / CC: ~5 min)

**Priority:** P2

**Depends on:** Stripe client refactor (HTTP status preservation + request timeouts) — shipped in PR #184.

**Status (2026-06-25):** NOT DONE. `stripe::init()` (`lit-api-server/src/stripe.rs:345-378`) still only checks env-var presence + local test-key format; no `validate_key()` / `GET /v1/balance` call exists.

## Enforce `max_get_keys_count` in handle_ops.rs

**What:** The `max_get_keys_count` field exists on `Client`, is configurable via chain config, and exposed via the config endpoint — but key-returning handlers in `handle_ops.rs` (`GetPrivateKey`, `GetLitActionPrivateKey`, `GetLitActionPublicKey`, `GetLitActionWalletAddress`) never check it.

**Why:** The field is cosmetic. A Lit Action could request unlimited key operations, bypassing the intended limit.

**How to fix:** Each key handler in `lit-api-server/src/actions/client/handle_ops.rs` should increment a counter on `ExecutionState` and bail if it exceeds `self.max_get_keys_count`. The counter field may need to be added to `ExecutionState` in `models.rs`.

**Risk:** Could break existing Lit Actions that exceed the (currently unenforced) limit. Consider logging a warning first, then enforcing in a follow-up.

**Added:** 2026-03-26 via /plan-eng-review on branch GTC6244/chain-config-expand

**Status (2026-06-25):** NOT DONE. `max_get_keys_count` lives on `Client` (`actions/client/mod.rs:81`) but the four key handlers (`handle_ops.rs:117-154`) perform no count check, and `ExecutionState` (`models.rs:48-79`) has no counter field. (Contrast `IncrementFetchCount` at `handle_ops.rs:57-69`, which does enforce its limit.)

## K6 Security: contract group ID semantics for cross-account isolation test (Test 9)

- **Priority:** P1
- **Context:** Adversarial review flagged that group IDs in AccountConfig.sol appear account-local (per-account auto-incrementing), not globally unique. If true, Test 9 (cross-account isolation) in `k6/correctness/api-key-security.spec.ts` comparing raw `groupId`s is unreliable.
- **Investigation result (2026-06-25): CONFIRMED account-scoped.** `WritesFacet.sol` `addGroup` increments a per-account counter and assigns `group.metadata.id = account.groupCount` (`WritesFacet.sol:379-383`); `AppStorage.Account` holds its own `groupCount` + `groups` mapping (`AppStorage.sol:82-102`). So two accounts can both own a group with id `1`.
- **Remaining action:** Rewrite Test 9 to test isolation via name-based lookups (unique `k6-sec-group*-${K6_RUN_ID}` names) instead of raw integer ID comparison. Currently Test 9 (`api-key-security.spec.ts:412-436`) still uses `parseInt(g.id) === data.groupIdX`, which can false-negative when both accounts hold the same local ID.
- **Branch:** GTC6244/k6-api-key-security-tests
- **Status (2026-06-25):** PARTIAL — semantics investigation done; Test 9 rewrite still pending.

## CPL-267 Sovereign Mode: GC orphan prepared wallet keys

**What:** Add periodic cleanup sweep for TEE-prepared wallets that were never registered on-chain. After server generates a key for sovereign `createWallet` and returns derivation metadata, user wallet may abandon before signing `register_wallet_derivation`, leaving TEE with an un-tracked key.

**Why:** Unbounded growth of orphan keys in TEE state. Also a minor security concern (keys exist with no on-chain accountability).

**How to fix:** Track prepared-wallet state with timestamp in TEE persistent storage. Background task (every N hours) drops entries older than threshold (e.g., 24h) that have no on-chain derivation registered.

**Priority:** P2 (not blocking ship, but required before external users)

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-self-sovereign-mode

**Status (2026-06-25):** NOT DONE. No prepared-wallet state tracking or background GC task found.

## CPL-267 Sovereign Mode: Document cache staleness window

**What:** SDK + dashboard docs should state: "After a sovereign-mode write, reads on other server instances may show stale data for up to N seconds (polling interval)."

**Why:** Per-instance event listener (user chose over single-leader) means N independent views of chain state. Users hitting load-balanced servers can see brief staleness. Undocumented surprises become support tickets.

**How to fix:** Add to SDK README + dashboard user docs. Also add dashboard banner when detected listener lag > 30s.

**Priority:** P2 (pair with Phase 3 listener ship)

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-self-sovereign-mode

**Status (2026-06-25):** NOT DONE. No staleness-window docs in SDK/dashboard; no listener-lag banner (only the existing ABI-drift banner exists).

## CPL-267 Sovereign Mode: Billing bypass documentation

**What:** Admin writes in sovereign mode bypass Stripe billing guards (user's wallet pays gas directly to chain). Lit Action execution still charges via Stripe. Document this split explicitly in SDK + billing page.

**Why:** Billing logic is per-op today (see `stripe::` in lit-api-server); sovereign admin writes never hit those guards. Accounting split must be visible to ops and support, otherwise conversion-to-sovereign looks like "billing broken."

**How to fix:** Add note to `billing.md` or equivalent. Add sovereign-mode label to Stripe dashboard for per-account identification.

**Priority:** P3

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-self-sovereign-mode

**Status (2026-06-25):** NOT DONE. No `billing.md` (or equivalent) documents the bypass split; `lit-payments-app.md` mentions sovereign mode only re: `getBillingWalletAddress`.

## CPL-267 Sovereign Mode: 6-month adoption re-evaluation

**What:** At 6 months post-ship of sovereign mode, review adoption metrics. If <5% of active accounts have converted or started sovereign, open a design doc to evaluate: (a) pivot to Approach B signed intents, (b) sunset sovereign mode, (c) continue parallel.

**Why:** Driver was internal alignment, not customer demand. Codex outside voice flagged whole approach as potentially wrong first proof. We rejected pivot now for philosophical reasons, but committed to data-driven re-evaluation.

**Priority:** P3 (reminder, not urgent — date-based, no code to verify)

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-self-sovereign-mode

## Monitor: Keyboard Shortcuts
- **What:** Add keyboard shortcuts to the Lit Node Monitor: R to refresh, F to fund all critical, S to toggle settings panel.
- **Why:** Operators use this tool daily. Keyboard shortcuts reduce friction for the most common actions.
- **Effort:** S (CC: ~5 min)
- **Priority:** P3
- **Depends on:** Phase 1 and Phase 2 payer safety console features
- **Context:** Deferred during CEO review of the payer safety console plan. Avoid conflicts with browser shortcuts (Ctrl+R, etc.) — use single-key shortcuts only when no input field is focused.
- **Status (2026-06-25):** NOT DONE — was previously filed under "Completed" but verification found it unimplemented. The only keydown handler in `monitor/app.js:902-908` toggles accordions (Enter/Space); no R/F/S shortcuts exist.

## Monitor: Network Health Badge in Dropdown
- **What:** Show a colored dot (green/yellow/red) next to each network name in the network selector dropdown, based on aggregate payer pool health for that network.
- **Why:** Operators currently must switch to each network to check payer health. A badge gives cross-network awareness at a glance.
- **Effort:** M (CC: ~15 min)
- **Priority:** P2
- **Depends on:** Phase 1 health state logic
- **Context:** Deferred during CEO review. Main complexity: requires background polling of all networks' payer balances simultaneously (not just the selected network), which increases RPC calls. Consider polling non-selected networks at a lower frequency (e.g., every 2 minutes vs 30 seconds for the active network).
- **Status (2026-06-25):** NOT DONE — was previously filed under "Completed" but verification found it unimplemented. The network selector (`monitor/index.html:380-385`) is a plain `<select>`; no health dots, no multi-network background polling.

## Completed

> Verified implemented in code on 2026-06-25.

## Dashboard: initLogin null dereference — DONE (2026-06-25)
- **What:** Null check for `#login-api-key` element in `initLogin()` (`auth.js`).
- **Evidence:** `auth.js:718-728` — `initLogin()` now guards with `if (!apiKeyInput) { … return; }` before any `.value` access (comment cites `TODOS.md:3-9`).
- **Original priority:** P1. Found by adversarial review during PR1 ship.

## Dashboard: initActionRunner async getCode fallback — DONE (2026-06-25)
- **What:** Initialize `getCode`/`getParams` fallback closures before the `await import(CodeJar)`, not only in the catch block.
- **Evidence:** `runner.js:20-42` — `let getCode; let getParams;` are declared before the `try`, then assigned in both the try and catch branches, so they're always defined.
- **Original priority:** P3. Found by adversarial review during PR1 ship.

## Server returns 403 for management permission denials (not 500) — DONE (2026-06-25)
- **What:** Map contract permission reverts to 403 Forbidden instead of 500 in `account_management.rs`.
- **Evidence:** `lit-api-server/src/core/account_management.rs:36-56` adds `map_contract_error()` matching `NotAllowedTo*` / `NotMasterAccount` / `NoAccountAccess` revert patterns and returning `ApiStatus::forbidden(...)`; applied to `add_group`, `remove_group`, `add_action_to_group`, `remove_action_from_group`, `add_pkp_to_group`, `remove_pkp_from_group`, and `convert_to_chain_secured_account`.
- **Note:** Pattern-matches the stringified revert; a follow-up could decode the ABI revert selector directly (noted in code comment).
- **Original priority:** P1. Branch GTC6244/k6-api-key-security-tests.

## CPL-267 Sovereign Mode: blockchain_cache `invalidate_for_account_hash(hash)` primitive — DONE (2026-06-25)
- **What:** Hash-based cache invalidation for the Phase 3 event listener (only has the hashed apiKeyHash from chain event topics).
- **Evidence:** `lit-api-server/src/accounts/blockchain_cache.rs:283-305` — `pub async fn invalidate_for_account_hash(account_api_key_hash: U256)` (plus sibling `invalidate_for_hash`), consumed by the on-chain listener in `lit-api-server/src/account_events.rs`.
- **Original priority:** P1 (blocked Phase 3). Added 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-self-sovereign-mode.
