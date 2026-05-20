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
| `lit-api-server` | 22 source files | Providers, signers, middleware, types, EIP-712, `abigen!` |
| `lit-api-server/blockchain/rust_generator_and_deployer` | 8 files (~3.4k LoC generated) | Standalone cargo workspace — contract codegen + deployer binary |
| `lit-core` | 0 (only a comment) | Already declares `alloy = "0.12.5"` in `[workspace.dependencies]` but no crate consumes it |
| `lit-actions` | 0 | Not affected |

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

### Phase 4 — Signers + providers + middleware (+ all U256/H160 sites)
This is the highest-risk phase — touches signing and nonce semantics. Also absorbs all the H160/U256-boundary leaf files that Phase 2 had to defer.

- [ ] `src/accounts/signer_pool.rs` — `LocalWallet` + `SignerMiddleware` + `NonceManagerMiddleware` → alloy `PrivateKeySigner` + `ProviderBuilder::new().with_recommended_fillers().wallet(...)`
- [ ] `src/accounts/mod.rs` — type updates
- [ ] `src/accounts/signable_contract.rs` — `ContractCall` → alloy `CallBuilder`; `SignerMiddleware`/`NonceManagerMiddleware` stack swap
- [ ] `src/core/account_management.rs` — signer construction + H160/U256 throughout; remove the alloy→ethers bridge added in Phase 3
- [ ] `src/actions/client/op_code_helpers/private_keys.rs` — signer construction
- [ ] `src/utils/mod.rs`, `src/utils/parse_with_hash.rs` — H160/U256 → alloy `Address`/`U256`
- [ ] `src/core/v1/models/curve_type.rs` — `TryFrom<ethers::U256>` / `From<CurveType> for ethers::U256` impls → alloy
- [ ] `src/core/v1/helpers/api_status.rs` — `From<ethers::providers::ProviderError>` etc. → alloy error types
- [ ] `src/accounts/blockchain_cache.rs` — U256/H160 in cache key fns
- [ ] Verify nonce-manager parity: alloy's `NonceFiller` has different cache/recovery semantics vs. ethers `NonceManagerMiddleware` — document any behavioral differences

### Phase 5 — `abigen!` → `sol!` in lit-api-server
- [ ] `src/restart.rs` — `EthEvent` subscription → alloy `Filter`/`Event::watch`
- [ ] `src/accounts/contracts/account_config_contract.rs` — regenerate bindings via `sol!` from existing ABI JSON

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

### Phase 7 — Drop ethers
- [ ] Remove `ethers` + `ethers-providers` from `lit-api-server/Cargo.toml`
- [ ] Remove `ethers` from `lit-core/Cargo.toml` workspace deps
- [ ] Remove `ethers` from `rust_generator_and_deployer/Cargo.toml`
- [ ] Delete these entries from `deny.toml` ignore list:
  - [ ] `RUSTSEC-2023-0071`
  - [ ] `RUSTSEC-2026-0049`
  - [ ] `RUSTSEC-2026-0098`
  - [ ] `RUSTSEC-2026-0099`
  - [ ] `RUSTSEC-2026-0104`
- [ ] Update `deny.toml` history comment with the snapshot diff
- [ ] `cargo deny check advisories` clean (modulo remaining unrelated advisories)
- [ ] `cargo tree | grep -E '(ethers|rustls-webpki 0\.10|rsa)' ` returns nothing in any workspace

## Risk register

- **Nonce-manager semantics (Phase 4):** alloy's `NonceFiller` resyncs from chain on RPC error; ethers' `NonceManagerMiddleware` cached aggressively. Document any prod-relevant divergence.
- **EIP-712 signature output (Phase 3):** confirm byte-identical signature for a known payload before cutover.
- **Diamond facet bindings (Phase 6):** the generated 1.3k-line file (`c_diamond_cut_facet.rs`) may contain manual tweaks — diff carefully against a fresh regeneration before committing.
- **`legacy` feature behavior:** ethers' `legacy` feature affected tx-type selection (pre-EIP-1559). Alloy defaults to EIP-1559; verify any chains in `NodeConfig.*.toml` that require legacy tx still work.
