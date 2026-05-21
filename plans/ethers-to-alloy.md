# ethers → alloy migration

## Motivation

The `ethers-rs` crate is unmaintained (frozen at 2.0.x; the maintainers
explicitly redirect new users to alloy). It pulls in old transitive deps that
account for **5 of the 9 vulnerability-class advisories** currently ignored in
`deny.toml`:

- `RUSTSEC-2023-0071` — rsa Marvin Attack timing sidechannel (transitive via ethers; no upstream fix coming)
- `RUSTSEC-2026-0049` — rustls-webpki CRL distribution-point matching (rustls 0.21 pinned by ethers)
- `RUSTSEC-2026-0098` — rustls-webpki URI name constraints (same chain)
- `RUSTSEC-2026-0099` — rustls-webpki wildcard name constraints (same chain)
- `RUSTSEC-2026-0104` — rustls-webpki CRL parsing panic (same chain)

Removing ethers also lets us drop the `legacy` feature flag and modernize the
EIP-712 / signer / provider stack on a maintained codebase.

## Surface area

| Workspace | Files touching ethers | Notes |
|---|---|---|
| `lit-api-server` | 22 Rust source files + Cargo manifest/lockfile | Providers, signers, middleware, types, EIP-712, `abigen!` |
| `lit-api-server/blockchain/rust_generator_and_deployer` | 8 Rust files (~3.4k LoC generated) + Cargo manifest/lockfile | Standalone cargo workspace — contract codegen + deployer binary |
| `lit-core` | 0 Rust imports; 1 workspace manifest dep; 1 explanatory comment | No code consumes `ethers`, but `[workspace.dependencies]` still declares it and must be removed in the final Rust dependency-drop phase |
| `lit-actions` | Bundled/runtime `ethers.js` API, not `ethers-rs` | Separate JS/runtime API surface; not part of the RustSec-driven `ethers-rs` removal unless we intentionally make a breaking Lit Actions API change |
| `lit-static` / `examples` / docs / k6 | `ethers.js` examples, browser SDK/dashboard code, Hardhat scripts, docs snippets | Separate JS cleanup/follow-up; Alloy is Rust-only, so these need an explicit replacement strategy (likely viem/native helpers) rather than being folded into the Rust migration |

## Target version

`alloy = "1.0"` (cargo will resolve to latest 1.x — currently 1.8.3, released
2026-03-27). 2.0.x is too fresh (~1 month old) to absorb breaking-change risk
during a migration of this size; we can bump to 2.x in a follow-up once the
ethers removal has been in prod for a release cycle.

## Phases

Each phase is a separate PR. Done = lands on `next` with green CI.

### Phase 1 — Add alloy alongside ethers
- [x] Bump `lit-core` workspace dep: `alloy = "0.12.5"` → `alloy = "1.0"` with features `["eip712", "sol-types", "signer-local", "providers", "contract", "rpc-types", "network"]`
- [x] Add `alloy` to `lit-api-server/Cargo.toml` via the workspace dep
- [x] `cargo check` clean in `lit-api-server` (both crates still build, ethers + alloy coexist)
- [x] No behavior change; ethers still in tree

### Phase 2 — Leaf util migrations (keccak256-only)
Truly leaf: files whose ethers usage doesn't cross the module boundary.

- [x] `src/dstack/v1/mod.rs` — `ethers::utils::keccak256` → `alloy::primitives::keccak256` (added `.0` to unpack `B256` into `[u8; 32]`)
- [x] `src/core/v1/guards/billing_auth.rs` — `keccak256` (no signature change needed; downstream uses `hex::encode` via `AsRef<[u8]>`)
- [x] `cargo check` clean in lit-api-server
- [x] Existing tests in migrated modules pass (`billing_auth::tests`, `dstack::v1::dstack::tests`)

**Files moved out of Phase 2** — after inspection, these all have `H160`/`U256` (or an ethers error type) in their **public API** and so migration must be batched with their callers. They cleanly belong to later phases:

- `src/utils/mod.rs`, `src/utils/parse_with_hash.rs` → bundle with Phase 4 (callers are `account_management.rs` / `core/mod.rs`, which are Phase 4 anyway)
- `src/core/v1/models/curve_type.rs` → has `TryFrom<ethers::U256>` and `From<CurveType> for ethers::U256` impls used cross-module; bundle with Phase 4
- `src/core/v1/helpers/api_status.rs` → has `From` impls for `ethers::providers::ProviderError` etc. used by `?` everywhere; bundle with Phase 4
- `src/accounts/decode_revert.rs` → takes `ethers::contract::ContractError<impl Middleware>` and calls `AccountConfigErrors::decode` (abigen-generated); bundle with Phase 5
- `src/accounts/blockchain_cache.rs` → cache key fns take `U256`/`H160` in public signatures called by the accounts layer; bundle with Phase 4

### Phase 3 — EIP-712
- [x] Add `dyn-abi` feature to alloy (needed for the runtime/dynamic typed-data shape that mirrors the ethers `TypedData` API; alloy's `sol!`-based static-struct approach doesn't fit a server that validates client-supplied schemas)
- [x] `src/core/eip712.rs` — replaced `ethers::core::types::transaction::eip712::{EIP712Domain, Eip712, Eip712DomainType, TypedData}` with `alloy::dyn_abi::TypedData` for the digest, plus a local `TypedDataSchemaView` for schema introspection (alloy's `Resolver` has private fields, so we deserialize the `types`/`primaryType`/`domain`/`message` slice ourselves and feed the same JSON into `TypedData` for the digest)
- [x] `RecoveryMessage`/`Signature::recover(...)` → `alloy::primitives::Signature::recover_address_from_prehash(&digest)`
- [x] **Cross-impl parity test** (`cross_impl_parity_ethers_signed_verifies_under_alloy`): ethers `LocalWallet` signs canonical TypedData → alloy verifier recovers the same address byte-for-byte. Confirms zero digest divergence between the two libs. This test stays in tree through Phase 6 and is deleted with the ethers dep in Phase 7.
- [x] Caller bridges: `src/core/v1/guards/billing_auth.rs` (one `.as_bytes()` → `.as_slice()`), `src/core/account_management.rs::convert_to_chain_secured_account` (one `H160::from_slice(signer.as_slice())` bridge — the other two callers only Debug-print the signer, no bridge needed)
- [x] All 19 `core::eip712::tests::*` pass; full 227-test lib suite passes; clippy clean

**Out-of-Phase-3 finding:** `src/accounts/signable_contract.rs` was originally listed under Phase 3 but on inspection it contains zero EIP-712 code — it's entirely `LocalWallet`/`SignerMiddleware`/`NonceManagerMiddleware`/`ContractCall`. Moved into Phase 4 where it belongs.

### Phase 4 — U256/H160 boundary migration + caller bridges
Migrates the H160/U256 boundary files and the accounts/account_management public APIs to alloy types. The signer/middleware/contract stack stays on ethers because the abigen-generated `AccountConfig<M>` requires `M: ethers::Middleware`; that swap moves to Phase 5 where the contract bindings get regenerated via `sol!` and the dependency inverts.

- [x] `src/utils/alloy_ethers.rs` — new bridge module with alloy↔ethers `U256`/`Address` helpers (removed in Phase 5 once the abigen layer is gone)
- [x] `src/utils/mod.rs`, `src/utils/parse_with_hash.rs` — H160/U256 → alloy `Address`/`U256`
- [x] `src/core/v1/models/curve_type.rs` — removed unused `TryFrom<ethers::U256>` / `From<CurveType> for ethers::U256` impls (no external callers; alloy's blanket `TryFrom` impl on `U256` conflicted with a hand-rolled `From`)
- [x] `src/core/v1/helpers/api_status.rs` — replaced `From<ethers::abi::ethereum_types::FromStrRadixErr>` with alloy's `ruint::ParseError`; dropped unused `From<ethers::providers::ProviderError>` and `From<ethers::utils::ConversionError>` (no callers)
- [x] `src/accounts/blockchain_cache.rs` — cache-key fns take alloy `U256`/`Address`; switched address rendering from ethers' lowercase `Debug` to `{:#x}` so keys stay deterministic
- [x] `src/actions/client/op_code_helpers/private_keys.rs` — `LocalWallet` → `PrivateKeySigner`
- [x] `src/accounts/mod.rs` — public API now takes/returns alloy `Address`/`U256`; internally bridges to ethers at every contract call site via `ae_addr` / `ae_u256` / `ea_u256`. Removes the alloy→ethers `H160::from_slice(signer.as_slice())` bridge added in Phase 3
- [x] `src/core/account_management.rs` — pubsig & internals on alloy types; wallet construction via `PrivateKeySigner`; `accounts::Metadata.id` comparisons keep `ethers::types::U256::zero()` because `Metadata` is still an abigen-generated struct (Phase 5 regenerates it)
- [x] Signer-pool / signable-contract internals **left on ethers** for Phase 5 — see header note above. The `SignerHandle.address` field stays `ethers::H160` since no alloy-typed caller touches it directly today.
- [x] 217 lib tests pass; clippy clean

### Phase 5 — `abigen!` → `sol!` in lit-api-server
Pairs the contract-binding regeneration with the signer/middleware stack swap, since the two are tightly coupled (alloy contracts take alloy providers; `sol!`-generated bindings replace the abigen `M: ethers::Middleware` constraint with alloy `Provider`).

- [ ] `src/restart.rs` — `EthEvent` subscription → alloy `Filter`/`Event::watch`
- [ ] `src/accounts/contracts/account_config_contract.rs` — regenerate bindings via `sol!` from existing ABI JSON
- [ ] `src/accounts/signer_pool.rs` — `LocalWallet` + `SignerMiddleware` + `NonceManagerMiddleware` → alloy `PrivateKeySigner` + `ProviderBuilder::new().with_recommended_fillers().wallet(...)`
- [ ] `src/accounts/signable_contract.rs` — `ContractCall` → alloy `CallBuilder`; `SignerMiddleware`/`NonceManagerMiddleware` stack swap
- [ ] `src/accounts/decode_revert.rs` — switch from ethers `ContractError` to alloy's equivalent
- [ ] Remove `src/utils/alloy_ethers.rs` and all `ae_*` / `ea_*` call sites in `accounts/mod.rs`
- [ ] `src/core/account_management.rs::metadata_to_item` — drop the temporary `ethers::types::U256::zero()` comparison once `Metadata.id` is alloy `U256`
- [ ] Verify nonce-manager parity: alloy's `NonceFiller` has different cache/recovery semantics vs. ethers `NonceManagerMiddleware` — document any behavioral differences

### Phase 6 — rust_generator_and_deployer (separate cargo workspace)
Largest single PR but mostly mechanical regeneration.

- [ ] Bump `rust_generator_and_deployer/Cargo.toml` to alloy
- [ ] Rewrite `src/bin/contract_generator.rs` to emit `sol!` macro invocations instead of `abigen!`
- [ ] Rewrite `src/bin/contract_deployer.rs` — `ContractFactory` → alloy `DeploymentBuilder`
- [ ] Rewrite `src/deployer/deploy.rs` + `src/deployer/diamond.rs`
- [ ] Regenerate all 6 diamond facet bindings:
  - [ ] `c_diamond_cut_facet.rs`
  - [ ] `c_diamond_init.rs`
  - [ ] `c_diamond_loupe_facet.rs`
  - [ ] `c_diamond_loupe_facet_no_erc165.rs`
  - [ ] `c_diamond_multi_init.rs`
  - [ ] `c_ownership_facet.rs`

### Phase 7 — Drop Rust `ethers-rs`
- [ ] Remove `ethers` + `ethers-providers` from `lit-api-server/Cargo.toml`
- [ ] Remove `ethers` from `lit-core/Cargo.toml` workspace deps (no Rust code imports it; this is a stale workspace dependency only)
- [ ] Remove `ethers` from `rust_generator_and_deployer/Cargo.toml`
- [ ] Delete `ethers` / `ethers-providers` entries from the affected `Cargo.lock` files after dependency resolution
- [ ] Delete the Phase 3 cross-implementation parity test in `lit-api-server/src/core/eip712.rs` (`cross_impl_parity_ethers_signed_verifies_under_alloy`), since it intentionally imports `ethers` until the final Rust drop
- [ ] Verify `git grep -n -E 'ethers::|use ethers|abigen!|ethers-providers|^ethers[[:space:]]*=' -- ':*.rs' ':*.toml' ':!**/target/**'` only returns allowed historical docs/comments, not build inputs
- [ ] Delete these entries from `deny.toml` ignore list:
  - [ ] `RUSTSEC-2023-0071`
  - [ ] `RUSTSEC-2026-0049`
  - [ ] `RUSTSEC-2026-0098`
  - [ ] `RUSTSEC-2026-0099`
  - [ ] `RUSTSEC-2026-0104`
- [ ] Update `deny.toml` history comment with the snapshot diff
- [ ] `cargo deny check advisories` clean (modulo remaining unrelated advisories)
- [ ] `cargo tree | grep -E '(ethers|rustls-webpki 0\.10|rsa)' ` returns nothing in each Rust workspace:
  - [ ] `lit-core`
  - [ ] `lit-api-server`
  - [ ] `lit-api-server/blockchain/rust_generator_and_deployer`

### Phase 8 — Inventory / replace non-Rust `ethers.js` surfaces
This is a separate cleanup from the RustSec-driven `ethers-rs` migration. Alloy does not replace browser/Node `ethers.js`, and Lit Actions currently exposes `ethers.js` as a runtime global, so this phase should start with an API-compat decision rather than a blind removal.

- [ ] Decide whether `lit-actions` should continue exposing `ethers.js` as a supported global. If not, plan the breaking-change window, replacement helper/API, and docs migration.
- [ ] `lit-actions/ext/js/00_ethers.js`, `05_globalsDocs.js`, `99_patches.js`, `server/cdn_module_loader.rs`, generated docs/types — either keep as an explicit supported runtime API or replace with the chosen JS alternative.
- [ ] `lit-static/core_sdk.js`, `wallet_connect.js`, `tx_lifecycle.js`, `dapps/dashboard/*`, `dapps/monitor/*` — replace browser `ethers.js` usage with the chosen JS library/native helpers, or explicitly document why these remain on `ethers.js`.
- [ ] `examples/*` and `lit-api-server/blockchain/lit_node_express` Hardhat scripts/package manifests — replace direct `ethers` dependencies/usages where practical; note that Hardhat's `hre.ethers` plugin usage may require a larger tooling migration.
- [ ] Docs / README / k6 snippets — update examples once the runtime/static/example replacements land.
- [ ] Verification for JS cleanup: `git grep -n -i 'ethers' -- ':!**/node_modules/**' ':!**/target/**'` returns only intentionally retained references.

## Risk register

- **Nonce-manager semantics (Phase 4):** alloy's `NonceFiller` resyncs from chain on RPC error; ethers' `NonceManagerMiddleware` cached aggressively. Document any prod-relevant divergence.
- **EIP-712 signature output (Phase 3):** confirm byte-identical signature for a known payload before cutover.
- **Diamond facet bindings (Phase 6):** the generated 1.3k-line file (`c_diamond_cut_facet.rs`) may contain manual tweaks — diff carefully against a fresh regeneration before committing.
- **`legacy` feature behavior:** ethers' `legacy` feature affected tx-type selection (pre-EIP-1559). Alloy defaults to EIP-1559; verify any chains in `NodeConfig.*.toml` that require legacy tx still work.
