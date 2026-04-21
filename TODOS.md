# TODOS

## Dashboard: initLogin null dereference
- **What:** Add null check for `#login-api-key` element in `initLogin()` (`auth.js`). Currently crashes if the element is absent.
- **Why:** Every other `getElementById` in the codebase uses optional chaining. This is the only unguarded access — will throw if the login panel HTML changes.
- **Effort:** XS (CC: ~2 min)
- **Priority:** P1
- **Depends on:** None
- **Context:** Found by adversarial review during PR1 ship. Pre-existing in the monolith. Fix: `if (apiKeyInput && getApiKey()) apiKeyInput.value = '';`

## Dashboard: setOnAuthReady fires before init* complete
- **What:** Move `setOnAuthReady(...)` call inside `init()` after all `init*` calls, instead of at module evaluation time in `app.js`.
- **Why:** When a user is already logged in from a previous session, `_onAuthReady` fires during `initLogin`, before `initWallets/initGroups/initActions/initKeys` have attached their button listeners. Load functions work fine but button-disable logic hasn't run yet, allowing brief duplicate-click window.
- **Effort:** XS (CC: ~2 min)
- **Priority:** P2
- **Depends on:** None
- **Context:** Introduced by module refactor (PR1). Low risk but a real ordering hazard.

## Dashboard: _confirmResolve promise race in ui-utils.js
- **What:** Guard `confirmDelete()` to reject or queue if a confirm dialog is already pending. Currently a second concurrent call overwrites `_confirmResolve`, permanently leaking the first promise.
- **Why:** Any `await confirmDelete(...)` caller that loses the race hangs silently forever. The `showActionProgress` non-dismissible modal reduces the window but doesn't eliminate it.
- **Effort:** S (CC: ~5 min)
- **Priority:** P2
- **Depends on:** None
- **Context:** Pre-existing in the monolith. Found by adversarial review during PR1 ship.

## Dashboard: msOutside listener accumulation in groups.js
- **What:** Remove the `document` click listener (`msOutside`) when the multi-select modal closes, instead of relying on the `!wrap.isConnected` guard on next click.
- **Why:** Opening and closing modals 10+ times without a subsequent document click accumulates orphan listeners on `document`. Memory/performance leak in long sessions.
- **Effort:** S (CC: ~5 min)
- **Priority:** P3
- **Depends on:** None
- **Context:** Pre-existing in the monolith. Found by adversarial review during PR1 ship.

## Dashboard: Form values read after closeModal in keys.js
- **What:** In `openUsageKeyModal` save handler, collect all form values (group IDs, permission checkboxes) *before* calling `closeModal()`.
- **Why:** Currently fragile — `closeModal` only hides the overlay but doesn't clear DOM. If `closeModal` is ever changed to clear innerHTML, `getSelectedGroupIds` would return empty arrays, silently creating a key with zero permissions.
- **Effort:** XS (CC: ~2 min)
- **Priority:** P3
- **Depends on:** None
- **Context:** Pre-existing in the monolith. Found by adversarial review during PR1 ship.

## Dashboard: initActionRunner async getCode fallback
- **What:** Initialize `getCode` and `getParams` fallback closures before the `await import(CodeJar)`, not only in the catch block.
- **Why:** On slow connections, if a user clicks execute before CDN import resolves, `getCode`/`getParams` are `undefined` (declared with `let`). The inline fallback `codeEl?.textContent` works but is a fragile pattern.
- **Effort:** XS (CC: ~2 min)
- **Priority:** P3
- **Depends on:** None
- **Context:** Pre-existing pattern in the monolith. Found by adversarial review during PR1 ship.

## P2: Startup Stripe key validation

**What:** Add `stripe::validate_key()` that calls `GET /v1/balance` at startup. Auth errors (401) are fatal (exit). Availability errors (5xx, timeout) are graceful (billing disabled, retry on first billing request).

**Why:** Currently `stripe::init()` only checks if env vars are present and non-empty. If the keys are invalid (revoked, wrong environment), billing silently fails on the first user request instead of at startup.

**Pros:** Immediate feedback on bad keys. Prevents silent billing failures. Catches test keys accidentally used in production (Stripe returns different key prefixes for test vs live).

**Cons:** Adds a network call at startup (~200ms). Requires the Stripe client refactor (preserve HTTP status, add timeouts) which shipped in PR #184.

**Context:** Identified during CEO plan review (2026-03-26). Codex flagged that the original Stripe client (`stripe_get`/`stripe_post` helpers) throws away HTTP status codes and uses a no-timeout reqwest::Client, making auth/availability distinction unreliable. The client refactor (PR #184) fixes that prerequisite.

**Effort:** S (human: ~2h / CC: ~5 min)

**Priority:** P2

**Depends on:** Stripe client refactor (HTTP status preservation + request timeouts) — shipped in PR #184.
## Enforce `max_get_keys_count` in handle_ops.rs

**What:** The `max_get_keys_count` field exists on `Client`, is configurable via chain config, and exposed via the config endpoint — but key-returning handlers in `handle_ops.rs` (`GetPrivateKey`, `GetLitActionPrivateKey`, `GetLitActionPublicKey`, `GetLitActionWalletAddress`) never check it.

**Why:** The field is cosmetic. A Lit Action could request unlimited key operations, bypassing the intended limit.

**How to fix:** Each key handler in `lit-api-server/src/actions/client/handle_ops.rs` should increment a counter on `ExecutionState` and bail if it exceeds `self.max_get_keys_count`. The counter field may need to be added to `ExecutionState` in `models.rs`.

**Risk:** Could break existing Lit Actions that exceed the (currently unenforced) limit. Consider logging a warning first, then enforcing in a follow-up.

**Added:** 2026-03-26 via /plan-eng-review on branch GTC6244/chain-config-expand
## K6 Security Tests

- **Investigate contract group ID semantics for cross-account isolation test**
  **Priority:** P1
  **Context:** Adversarial review (Codex structured + adversarial) flagged that group IDs in AccountConfig.sol appear to be account-local (per-account auto-incrementing), not globally unique. If true, Test 9 (cross-account isolation) in `k6/correctness/api-key-security.spec.ts` passes Account A's `groupIdX` under Account B's auth, which would resolve to Account B's own group with that local ID — not Account A's group. The `listGroups` ID comparison is also unreliable under this model.
  **Action:** Read `WritesFacet.sol` and `AccountConfig.sol` to confirm whether group IDs are account-scoped. If so, rewrite Test 9 to test isolation via name-based lookups or by verifying that Account B cannot see Account A's group names in `listGroups`.
  **Branch:** GTC6244/k6-api-key-security-tests
  **Noticed:** 2026-03-26

- **Server should return 403 for management permission denials (not 500)**
  **Priority:** P1
  **Context:** Management operations (`addGroup`, `removeGroup`, `createWallet`, `addActionToGroup`, `addPkpToGroup`, `removePkpFromGroup`) return 500 (contract revert) when a usage key lacks the required permission. The security tests assert 403 as a forcing function. Until the server maps contract permission reverts to `ApiStatus::forbidden()` in `account_management.rs`, the negative tests for groups 2-7 and 13 will fail.
  **Action:** In `lit-api-server/src/core/account_management.rs`, catch contract permission revert errors and return 403 Forbidden instead of 500 Internal Server Error.
  **Branch:** GTC6244/k6-api-key-security-tests
  **Noticed:** 2026-03-26

## CPL-267 Phase 2: newAccount access control for sovereign self-serve

**What:** Relax the `OnlyApiPayerOrOwner` guard on `AccountConfig.newAccount` so a user-owned wallet can create an unmanaged (`managed=false`) self-sovereign account. Today the check blocks every non-api_payer caller, which is correct for server-managed accounts but blocks the whole Phase 2 direct-write path.

**Why:** Without this, sovereign `newAccount` always reverts with `OnlyApiPayerOrOwner(0x…)`. The client-side SDK branch is already wired and decoded end-to-end (see `core_sdk.js::newAccount` sovereign branch on `feature/cpl-267-phase-2-sovereign-accounts`); only the contract gate stands in the way.

**How to fix:** Add a branch in `AccountConfig.newAccount`: if `managed == false` and `creatorWalletAddress == msg.sender`, skip the api_payer check. Keep managed accounts behind the existing guard. Consider rate-limiting per-address to avoid spam.

**Priority:** P1 (blocks Phase 2 demo)

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-phase-2-sovereign-accounts

## CPL-267 Phase 2: POST /prepare_sovereign_wallet TEE-prepare endpoint

**What:** Add a new endpoint that generates a PKP inside the TEE, persists derivation metadata, and returns `{ pkp_id, derivation_path, expires_at }`. Hybrid `createWallet` in sovereign mode calls this first, then the user signs `registerWalletDerivation` on-chain to finalize.

**Why:** PKPs must be generated inside the TEE (the user's browser can't mint one). But registration should land on-chain from the user's wallet so the server is not in the trust boundary for account linkage. Without this endpoint, sovereign `createWallet` 404s at step 1.

**How to fix:** New endpoint `/core/v1/prepare_sovereign_wallet` accepting `{ api_key }`, generating a PKP in TEE, storing `{ pkp_id → { derivation_path, expires_at, registered: false } }` in persistent state, returning pkp_id + derivation_path. Pair with the "GC orphan prepared wallet keys" TODO.

**Priority:** P1 (blocks Phase 2 createWallet)

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-phase-2-sovereign-accounts

## CPL-267 Phase 2: API → sovereign conversion dashboard UX

**What:** Add a one-way "convert this account to sovereign" flow in the dashboard. Requires a wallet-connect step, an explicit confirmation modal explaining consequences (server can no longer act on your behalf; all admin ops need your wallet signature; Lit Action billing continues), and a contract write that flips `managed` to `false` for the account.

**Why:** Users who currently run API-mode accounts can't migrate today without manually rotating. One-way conversion is the sanctioned path; pairing it with an explanatory modal prevents support tickets and accidental lockouts.

**How to fix:** Need a contract function like `setAccountManaged(hash, false)` gated to the current owner. Dashboard adds a "Convert to sovereign" button in account settings that runs through the preview+confirm pipeline like any other sovereign write.

**Priority:** P2 (nice-to-have for Phase 2 ship; parallel to contract work)

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-phase-2-sovereign-accounts

## CPL-267 Sovereign Mode: blockchain_cache needs `invalidate_for_account_hash(hash)` primitive

**What:** Add `invalidate_for_account_hash(api_key_hash: Bytes32)` to `lit-api-server/src/accounts/blockchain_cache.rs`. Existing invalidation functions (`invalidate_for_account`, `invalidate_for_key`) take raw api_key and hash internally, but the Phase 3 event listener only has the hashed apiKeyHash from chain event topics.

**Why:** Blocker for Phase 3 event listener in CPL-267 sovereign mode. Without this primitive, listener can't invalidate cache entries for direct-write transactions.

**How to fix:** Factor out the hash-based invalidation path from `invalidate_for_account` into a new public function. Existing callers keep their raw-key signature; listener calls the hash variant directly.

**Priority:** P1 (blocks Phase 3)

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-self-sovereign-mode

## CPL-267 Sovereign Mode: GC orphan prepared wallet keys

**What:** Add periodic cleanup sweep for TEE-prepared wallets that were never registered on-chain. After server generates a key for sovereign `createWallet` and returns derivation metadata, user wallet may abandon before signing `register_wallet_derivation`, leaving TEE with an un-tracked key.

**Why:** Unbounded growth of orphan keys in TEE state. Also a minor security concern (keys exist with no on-chain accountability).

**How to fix:** Track prepared-wallet state with timestamp in TEE persistent storage. Background task (every N hours) drops entries older than threshold (e.g., 24h) that have no on-chain derivation registered.

**Priority:** P2 (not blocking ship, but required before external users)

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-self-sovereign-mode

## CPL-267 Sovereign Mode: Document cache staleness window

**What:** SDK + dashboard docs should state: "After a sovereign-mode write, reads on other server instances may show stale data for up to N seconds (polling interval)."

**Why:** Per-instance event listener (user chose over single-leader) means N independent views of chain state. Users hitting load-balanced servers can see brief staleness. Undocumented surprises become support tickets.

**How to fix:** Add to SDK README + dashboard user docs. Also add dashboard banner when detected listener lag > 30s.

**Priority:** P2 (pair with Phase 3 listener ship)

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-self-sovereign-mode

## CPL-267 Sovereign Mode: 6-month adoption re-evaluation

**What:** At 6 months post-ship of sovereign mode, review adoption metrics. If <5% of active accounts have converted or started sovereign, open a design doc to evaluate: (a) pivot to Approach B signed intents, (b) sunset sovereign mode, (c) continue parallel.

**Why:** Driver was internal alignment, not customer demand. Codex outside voice flagged whole approach as potentially wrong first proof. We rejected pivot now for philosophical reasons, but committed to data-driven re-evaluation.

**Priority:** P3 (reminder, not urgent)

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-self-sovereign-mode

## CPL-267 Sovereign Mode: Billing bypass documentation

**What:** Admin writes in sovereign mode bypass Stripe billing guards (user's wallet pays gas directly to chain). Lit Action execution still charges via Stripe. Document this split explicitly in SDK + billing page.

**Why:** Billing logic is per-op today (see `stripe::` in lit-api-server); sovereign admin writes never hit those guards. Accounting split must be visible to ops and support, otherwise conversion-to-sovereign looks like "billing broken."

**How to fix:** Add note to `billing.md` or equivalent. Add sovereign-mode label to Stripe dashboard for per-account identification.

**Priority:** P3

**Added:** 2026-04-21 via `/plan-eng-review` on branch feature/cpl-267-self-sovereign-mode

## Completed
## Monitor: Keyboard Shortcuts
- **What:** Add keyboard shortcuts to the Lit Node Monitor: R to refresh, F to fund all critical, S to toggle settings panel.
- **Why:** Operators use this tool daily. Keyboard shortcuts reduce friction for the most common actions.
- **Effort:** S (CC: ~5 min)
- **Priority:** P3
- **Depends on:** Phase 1 and Phase 2 payer safety console features
- **Context:** Deferred during CEO review of the payer safety console plan. Avoid conflicts with browser shortcuts (Ctrl+R, etc.) — use single-key shortcuts only when no input field is focused.

## Monitor: Network Health Badge in Dropdown
- **What:** Show a colored dot (green/yellow/red) next to each network name in the network selector dropdown, based on aggregate payer pool health for that network.
- **Why:** Operators currently must switch to each network to check payer health. A badge gives cross-network awareness at a glance.
- **Effort:** M (CC: ~15 min)
- **Priority:** P2
- **Depends on:** Phase 1 health state logic
- **Context:** Deferred during CEO review. Main complexity: requires background polling of all networks' payer balances simultaneously (not just the selected network), which increases RPC calls. Consider polling non-selected networks at a lower frequency (e.g., every 2 minutes vs 30 seconds for the active network).
