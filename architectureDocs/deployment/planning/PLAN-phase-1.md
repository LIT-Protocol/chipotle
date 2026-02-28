# Phase 1: Prerequisites & Configuration

**Parent**: [PLAN.md](PLAN.md) | **Requirements**: [requirements.md](requirements.md)

## Goal

Establish attestation source, digest pinning, and deployment configuration. No cross-workflow dependencies within this phase.

**Production constraint** (NFR-2.1, NFR-2.2): Avoid vendor lock-in. Phala Cloud is OK for development; production SHALL depend on open dstack components only, not Phala-exclusive infra or tooling.

## Prerequisites

- None (first phase)

## Parallelizable Workflows

| Workflow | Tasks | Requirements | Parallelizable with |
|----------|-------|--------------|---------------------|
| **A** | Add `/attestation` and `/info` endpoints to lit-api-server. Gateway cannot serve attestation (quote, event_log, vm_config, report_data)—per [Phala Get Attestation](https://docs.phala.com/phala-cloud/attestation/get-attestation) it must come from the application. Expose app_compose or compose-hash via `/info`. | FR-1.1, FR-1.2, FR-1.3 | B, C, D |
| **B** | Pin images by digest in docker-compose; update deploy workflow to use `@sha256:` | DR-1.1, DR-1.2 | A, C, D |
| **C** | Verify KMS/RoT configurable (pcloud/derot); add dstack simulator support if missing | FR-2.1–FR-2.5 | A, B, D |
| **D** | **Onchain KMS on Base**: Deploy DstackApp; configure app creation for Onchain KMS; document Base contract/RPC | FR-2.6 | A, B, C |

## Workflow Details

### Workflow A: Attestation Mechanism (FR-1)

- **Constraint** (NFR-2.1, NFR-2.2): Production uses open DSTACK only. Trust Center is Phala-exclusive (dev only).
- **Decision**: The **application** is the attestation source. Per [Phala Get Attestation](https://docs.phala.com/phala-cloud/attestation/get-attestation), the gateway cannot serve attestation (quote, event_log, vm_config, report_data)—it must come from the application. The app mounts the dstack socket, fetches via `get_quote()`, and exposes attestation at `/attestation` and `/info`.
- **Implement** in lit-api-server:
  - `GET /attestation` — returns quote, event_log, vm_config, report_data (from dstack socket)
  - `GET /info` — returns app info including tcb_info.app_compose (or compose-hash)
- Local: dstack simulator (`DSTACK_SOCKET`). Dev/prod: app serves attestation; verifier fetches from `{base_url}/attestation` and `{base_url}/info`.
- Document `--expected-compose-hash` fallback for verifier when app_compose unavailable

### Workflow B: Docker Image Pinning (DR-1) and Sigstore (DR-1b)

- Update `docker-compose.phala.yml` to use `@sha256:` digests
- Update deploy workflow to substitute digest-pinned images (not `:sha` or `:latest`)
- **Add Sigstore** for container provenance: sign images in GitHub Actions builds. Links image digests to source code via GitHub-endorsed builds. See [Phala: Image Provenance](https://docs.phala.com/phala-cloud/attestation/verify-your-application#image-provenance).

### Workflow C: KMS / Root of Trust (FR-2)

- Verify Cargo feature flags: `pcloud` (dev), `derot` (production per NFR-2)
- Production default: `derot` (Onchain KMS on Base). `pcloud` (Cloud KMS) for dev only.
- Ensure local testing with appropriate RoT backend
- Add or verify dstack simulator support (e.g. `DSTACK_SIMULATOR_ENDPOINT`)

### Workflow D: Onchain KMS on Base (FR-2.6)

- **Deploy DstackApp on Base**: Use Phala Cloud dashboard "Onchain KMS" when creating app, or deploy custom DstackApp via [dstack repository](https://github.com/Dstack-TEE/dstack). Base contract: `0x2f83172A49584C017F2B256F0FB2Dca14126Ba9C`.
- **Configure governance**: Set DstackApp `owner` (wallet, multisig, timelock, or DAO) for compose-hash whitelist control.
- **Document**: Base RPC (`kms.dstack-base-prod7.phala.network`), contract address, and app-creation flow for Onchain KMS.
- **Build**: Production builds use `derot` feature; CVM requests keys from Onchain KMS after attestation.

## Files to Create/Modify

| File | Action | Workflow |
|------|--------|----------|
| lit-api-server | Modify — add `/attestation` and `/info` endpoints | A |
| `docker-compose.phala.yml` | Modify — `@sha256:` digests | B |
| `.github/workflows/deploy-phala.yml` | Modify — pin images by digest; add Sigstore signing for container provenance | B |
| lit-api-server (or relevant crate) | Modify — KMS/RoT, simulator | C |
| DstackApp on Base | Deploy or document use of shared contract | D |
| `docs/deployment/planning/` | Add Onchain KMS setup notes (Base contract, RPC, app creation) | D |

## Exit Criteria / Gate

- **FR-1 mechanism decided and implemented** — Required before Phase 2 (verifier needs attestation source)
- **Onchain KMS on Base configured** — DstackApp deployed or documented; app creation flow for production established
- Workflows B, C, D can complete in any order; A must complete for Phase 2 to start

**Deferred to Phase 4**: Custom domain design (FR-3) — design + documentation

## Next Phase

→ [PLAN-phase-2.md](PLAN-phase-2.md) (Verifier Crate)
