# TODOS

## Enforce `max_get_keys_count` in handle_ops.rs

**What:** The `max_get_keys_count` field exists on `Client`, is configurable via chain config, and exposed via the config endpoint — but key-returning handlers in `handle_ops.rs` (`GetPrivateKey`, `GetLitActionPrivateKey`, `GetLitActionPublicKey`, `GetLitActionWalletAddress`) never check it.

**Why:** The field is cosmetic. A Lit Action could request unlimited key operations, bypassing the intended limit.

**How to fix:** Each key handler in `lit-api-server/src/actions/client/handle_ops.rs` should increment a counter on `ExecutionState` and bail if it exceeds `self.max_get_keys_count`. The counter field may need to be added to `ExecutionState` in `models.rs`.

**Risk:** Could break existing Lit Actions that exceed the (currently unenforced) limit. Consider logging a warning first, then enforcing in a follow-up.

**Added:** 2026-03-26 via /plan-eng-review on branch GTC6244/chain-config-expand
