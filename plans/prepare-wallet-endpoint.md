# `prepare_wallet` — unsigned derived-wallet-address endpoint (Option A)

**Status:** Implemented + codex-reviewed (see §7).
**Scope:** Add a single new unauthenticated endpoint that returns a fresh `(wallet_address, derivation_path)` pair with **no owner signature**, so clients can obtain a PKP address before registering it on-chain and fold the whole owner ceremony into one signed bind UserOp. Unblocks [flows#532](https://github.com/LIT-Protocol/flows/issues/532) (new EVM wallet 3→1 prompts, first exchange connect 2→1).

Related: security issue [chipotle#575](https://github.com/LIT-Protocol/chipotle/issues/575) (public derivation paths / per-account-only uniqueness) — **pre-existing, not caused or worsened by this endpoint**, tracked separately.

---

## 1. Background — why this is a small change

The existing "mint" endpoints don't mint anything on-chain. `create_wallet_with_signature` (`lit-api-server/src/core/account_management.rs:238`) verifies an EIP-712 signature and then just calls `create_new_wallet()`, which:

1. `generate_unique_derivation_path()` → a random 256-bit path (keccak of 32 CSPRNG bytes, `utils/mod.rs:16`),
2. `get_client_key(path)` → deterministic secp256k1 secret from the TEE/dstack KDF (`dstack/v1/mod.rs`),
3. derives the EVM address,
4. returns `{ wallet_address, derivation_path }`.

The client then does the real authorization step itself: an on-chain `registerWalletDerivation(adminHash, wallet_address, derivation_path, …)` inside its owner-signed UserOp (`WritesFacet.sol:664`). The EIP-712 signature authorizes **nothing durable** — the code's own comment (`core/eip712.rs`) states a replay "just produces an extra unattached PKP … compute cost only." The signature is purely the API's auth shape and is the source of the extra WebAuthn prompt.

**Option A** removes that signature step: an unauthenticated endpoint that returns a fresh server-generated `(wallet_address, derivation_path)`. Same return shape as `create_wallet_with_signature`, minus the signature.

### Why server-generated path only (Option A, not B)

We deliberately do **not** accept a client-supplied `derivation_path` in this endpoint:

- **Low-entropy footgun:** paths are a global namespace. Two clients passing a weak path (e.g. `0x1`) would derive the **same** key. Server-side `generate_unique_derivation_path` guarantees 256-bit CSPRNG entropy.
- **Avoids widening issue #575:** client-supplied paths are exactly the cross-account collision vector in that issue. Keeping path generation server-side sidesteps it entirely.

A client-supplied-path lookup (Option B, needed for the #450-style recovery flow) is a **separate future endpoint** and should land only alongside the #575 contract fix.

---

## 2. Semantics to be explicit about (docs + code comments)

`prepare_wallet` is **not idempotent**, and callers must understand this:

1. **Every call returns a brand-new wallet.** Each request generates a fresh random path → fresh unique address. Retrying does **not** return the previous address; it produces another candidate.
2. **The response is ephemeral until registered.** A `(wallet_address, derivation_path)` that is never followed by an on-chain `registerWalletDerivation` is just a discarded keypair — it secures nothing and costs nothing (equivalent to a freshly generated keypair, same as today's `_with_signature` endpoints on replay). Register exactly once; treat un-registered responses as throwaway.
3. **No server-side dedup for concurrent callers.** Two systems that both "ensure the account has a wallet" will each get a **different** address and each fire a **different** bind UserOp → the account ends up with **two** wallets, not one shared wallet. If concurrent callers must converge on one wallet, they must coordinate client-side (check `listPkps`/existing registry first, or designate a single writer).

### Is there an accidental *collision*? No.

- Distinct random paths per call ⇒ distinct addresses ⇒ no shared key to collide on.
- Even in the pathological "register the same address twice" case, the contract reverts cleanly with `"PKP already registered"` (`WritesFacet.sol:681`) — no corruption. Concurrent `prepare_wallet` calls can't hit this because their addresses differ.

So: concurrent callers on the same account cannot accidentally register the *same* derivation. The only surprise is the non-idempotency above (two wallets instead of one), which the docs must state plainly.

---

## 3. Implementation

### 3.1 Handler — `lit-api-server/src/core/account_management.rs`

Add `prepare_wallet()` next to `create_wallet_with_signature`. Reuses the existing private `create_new_wallet()`; discards the secret (never returned).

```rust
/// Return a fresh derived wallet address + derivation path with NO owner
/// signature. The client is expected to register it on-chain itself via
/// `registerWalletDerivation` inside its own owner-signed UserOp — that
/// on-chain call is the real authorization boundary (see core::eip712 notes).
///
/// NOT IDEMPOTENT: every call returns a brand-new wallet. See PrepareWalletResponse
/// docs and docs/management/api_direct.mdx for the concurrency semantics.
pub async fn prepare_wallet() -> Result<PrepareWalletResponse, ApiStatus> {
    let (_public_key, wallet_address, _secret, derivation_u256) = create_new_wallet().await?;
    Ok(PrepareWalletResponse {
        wallet_address: bytes_to_0x_hex(wallet_address.as_slice()),
        derivation_path: format!("0x{:x}", derivation_u256),
    })
}
```

### 3.2 Response model — `lit-api-server/src/core/v1/models/response.rs`

Dedicated type (clearer OpenAPI than reusing the signature response):

```rust
/// Returned by `POST /prepare_wallet`. Same shape as
/// `CreateWalletWithSignatureResponse` but obtained with no owner signature.
/// The client MUST follow up with an on-chain
/// `registerWalletDerivation(adminHash, wallet_address, derivation_path, name, description)`;
/// until that lands the PKP exists in MPC but is registered to no account.
///
/// NOT IDEMPOTENT — each call returns a new wallet; see api_direct.mdx.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PrepareWalletResponse {
    pub wallet_address: String,
    /// 0x-prefixed lowercase hex (uint256). Pass verbatim to
    /// `registerWalletDerivation`'s `derivationPath` arg.
    pub derivation_path: String,
}
```

No request model: the endpoint takes no body (server generates everything).

### 3.3 Route — `lit-api-server/src/core/v1/endpoints/account_management.rs`

Unauthenticated (matches its `_with_signature` siblings — the ChainSecured ceremony has no API key at this point). POST (metered-style write semantics; also keeps it off prefetchers/link-previewers, per the `create_wallet` GET-deprecation note).

```rust
/// Return a fresh derived wallet address + derivation path — no signature, no API key.
/// Intended to collapse the ChainSecured owner ceremony into a single signed bind
/// UserOp: fetch the address here, then register it on-chain yourself.
///
/// NOT IDEMPOTENT: every call returns a new wallet. See docs/management/api_direct.mdx.
#[openapi(tag = "Account Management")]
#[post("/prepare_wallet")]
pub(super) async fn prepare_wallet() -> OpenApiResponse<PrepareWalletResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(account_management::prepare_wallet().await).into(),
    }
}
```

Add `PrepareWalletResponse` to the response `use` block.

### 3.4 Register route — `lit-api-server/src/core/v1/endpoints/mod.rs`

Add `prepare_wallet,` to the `openapi_get_routes_spec![...]` list.

---

## 4. Docs — `docs/management/api_direct.mdx`

- Add a `#### POST /prepare_wallet` section near `create_wallet_with_signature`, documenting: no auth, no body, response shape, the mandatory on-chain `registerWalletDerivation` follow-up, and the **non-idempotency / concurrency** semantics from §2 (verbatim intent).
- Cross-reference from the `create_wallet_with_signature` section that `prepare_wallet` is the no-signature equivalent that collapses the ceremony to one prompt.
- Note it returns the equivalent of a fresh keypair until registered (compute-only cost).

---

## 5. Tests

- Unit (`core/account_management.rs` or a test mod): `prepare_wallet` returns 0x-prefixed 20-byte address + 0x-prefixed uint256 path; two calls return **different** addresses (non-idempotency); the returned path canonicalizes identically to what the signing path (`pkp_id_to_derviation_path` / `u256_to_derviation_path`) expects. (dstack KDF may need the same test harness/mocking the existing wallet-creation tests use — match whatever `create_wallet_with_signature` tests do; if none exist, add a focused test that doesn't require live dstack, or gate behind the existing integration test setup.)
- Ensure OpenAPI spec builds (route compiles into `openapi_get_routes_spec!`).

---

## 6. Out of scope / follow-ups

- **Client-supplied-path lookup (Option B)** — for #450 recovery; land with the #575 contract fix.
- **Deprecating `create_wallet_with_signature` / `add_usage_api_key_with_signature`** — once flows migrates to `prepare_wallet` + client-generated usage keys, these become redundant. Not removed here to avoid breaking current clients.
- **#575 contract fix** (global first-owner binding) — tracked separately; independent of this endpoint.

---

## 7. Codex adversarial review — outcome

Ran `codex challenge` (high effort) against the working-tree diff. Three findings, all addressed:

1. **[P1] On-chain registration is not a robust authorization boundary (cross-account path hijack).** This is exactly chipotle#575 — pre-existing, and this endpoint does not introduce or widen it (paths + addresses are already public). Resolution: corrected the handler doc comment so it no longer implies the on-chain registration is airtight; it now states paths are public, explains the per-account uniqueness gap, and cross-references #575. Left the public API doc narrow (explains only that the signature is non-load-bearing) rather than publishing exploit detail for an unpatched hole.
2. **[P1] Unauthenticated, unthrottled dstack-KDF trigger (DoS).** Valid: unlike the `_with_signature` siblings there's no EIP-712 verification in front of the KDF call. Resolution: added the `CpuAvailable` load-shedding guard (same one `lit_action` uses) to `prepare_wallet`, so the endpoint sheds with 429 under CPU saturation.
3. **[P2] Docs overclaimed "no collision / clean revert".** Valid. Resolution: reworded the `Warning` block — collision is *cryptographically negligible* (not impossible), and the `"PKP already registered"` revert is explicitly per-account, not a cross-account guard.

Not changed: the endpoint stays unauthenticated by design (the ChainSecured ceremony has no API key at this point), matching its `_with_signature` siblings.
