# Phase 2: Verifier Crate

**Parent**: [PLAN.md](PLAN.md) | **Requirements**: [requirements.md](requirements.md)

## Goal

Build `verify-cvm` based on [dstack-verifier](https://github.com/Dstack-TEE/dstack/tree/master/verifier) (Dstack-TEE/dstack, Apache-2.0). Produces a standalone binary runnable via `cargo run -p verify-cvm -- [args]`.

**Base**: dstack-verifier already provides quote verification, event log verification, OS image hash verification (VR-1, VR-2 core). We extend with VR-3 (network), VR-4 (governance), base_url fetch, and `--skip-*` flags.

**Constraint (NFR-2)**: Verifier SHALL use open dstack tooling only (dcap-qvl, dstack-mr). No Phala Cloud API or Trust Center dependency for production.

**Onchain KMS on Base**: Production uses DstackApp on Base; VR-1.6, VR-2.5, VR-2.9, VR-4 are mandatory (compose-hash whitelist, OS whitelist, KMS MR, contract verification).

## Prerequisites

- **Phase 1 complete**, especially FR-1 (attestation mechanism decided and implemented)

## Parallelizable Workflows

| Workflow | Tasks | Requirements | Parallelizable with |
|----------|-------|--------------|---------------------|
| **A** | Integrate dstack-verifier (submodule or dep); add base_url fetch → VerificationRequest; CLI wrapper | FR-4.1 | B, C, D |
| **B** | Network layer: TLS fingerprint, evidence binding, CAA records, cert chain validation (extend dstack-verifier) | FR-4, VR-3 | A, C, D |
| **C** | Governance layer: contract addresses, permissions, ownership; DstackApp/DstackKms checks (extend dstack-verifier) | FR-4, VR-4 | A, B, D |
| **D** | CLI: `--skip-os`, `--skip-code`, `--skip-tls`, `--skip-governance`; Trust Center link (dev only); pass/fail per step | FR-4.3, FR-4.4 | A, B, C |

## Workflow Details

### Workflow A: dstack-verifier Integration

- Add dstack-verifier as dependency (git submodule or `[git]` dep from Dstack-TEE/dstack)
- Fetch attestation from `{base_url}/attestation` and `{base_url}/info` (or accept quote, event_log, app_compose directly)
- Build `VerificationRequest` from fetched data; invoke `CvmVerifier::verify`
- CLI wrapper: `verify-cvm --base-url URL` or `verify-cvm --verify report.json` (oneshot)
- dstack-verifier already covers: quote verification (dcap-qvl), event log, OS image hash, TCB status, app_info (compose hash)

### Workflow B: Network Layer (VR-3) — Extend

- TLS certificate fingerprint vs served cert
- Evidence files at `/evidences/` binding to quote (reportData)
- CAA DNS records
- Certificate chain validation

### Workflow C: Governance Layer (VR-4) — Extend (Onchain KMS on Base)

- Smart contract address verification (DstackApp, DstackKms) — Base: `0x2f83172A49584C017F2B256F0FB2Dca14126Ba9C`
- Contract permissions, ownership and upgrade mechanisms
- Compose-hash whitelist (DstackApp), OS whitelist (DstackKms), KMS aggregated MR
- Production: all governance checks mandatory (Onchain KMS required)

### Workflow D: CLI and Output

- `--skip-os`, `--skip-code`, `--skip-tls`, `--skip-governance` flags
- Trust Center link in output (optional; only when `--phala-dev`; omit for production)
- Pass/fail output per step

## Integration

- verify-cvm uses `dstack_verifier::CvmVerifier` for core verification; adds VR-3, VR-4 modules
- Unified CLI: `--base-url`, or `--verify report.json` (oneshot), or direct quote/event_log/app_compose
- Pass/fail output per step
- One-time use design

## Dependencies (NFR-2)

- **dstack-verifier** (Dstack-TEE/dstack, Apache-2.0) — base; includes dcap-qvl, dstack-mr, cc-eventlog
- **verify-cvm** — thin wrapper crate extending dstack-verifier with VR-3, VR-4, base_url fetch, `--skip-*`
- Phala Cloud API: dev only; not a production dependency

## Files to Create/Modify

| File | Action | Workflow |
|------|--------|----------|
| `dstack/` (git submodule) or `[patch]` / `[git]` dep | Add Dstack-TEE/dstack | A |
| `verify-cvm/` (Rust crate) | Create — depends on dstack_verifier; extends with VR-3, VR-4, CLI | A, B, C, D |
| `verify-cvm/README.md` | Create | D |
| Workspace `Cargo.toml` | Modify — add verify-cvm | D |

## Exit Criteria / Gate

- Verifier runs end-to-end (all layers)
- All VR steps implemented (or skippable via `--skip-*`)
- Binary produces pass/fail output per step

## Next Phase

→ [PLAN-phase-3.md](PLAN-phase-3.md) (Verification Automation)
