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
