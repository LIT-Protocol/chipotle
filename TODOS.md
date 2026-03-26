# TODOS

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
