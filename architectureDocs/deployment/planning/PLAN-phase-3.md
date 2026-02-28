# Phase 3: Verification Automation

**Parent**: [PLAN.md](PLAN.md) | **Requirements**: [requirements.md](requirements.md)

## Goal

Company-run verification and published results. Automated verification uses the verifier tool and runs after each CVM deployment; results are published to a company-maintained information repository. End users may optionally run the verifier themselves or trust the published results.

**Constraint (NFR-2)**: Support both dev (Phala Cloud) and production (DeRoT, self-hosted) deploy paths. Verification automation SHALL work for any dstack-compatible deployment; not tied to Phala-exclusive infra.

## Prerequisites

- **Phase 2 complete** — `verify-cvm` crate exists and runs end-to-end

## Parallelizable Workflows

| Workflow | Tasks | Requirements | Parallelizable with |
|----------|-------|--------------|---------------------|
| **A** | Create verification repository (location, format, CVM→output association) | FR-5.2, FR-5.3 | B |
| **B** | Add post-deploy verification job to deploy workflow; run verifier; publish results | FR-5.1, FR-5.4 | A |

## Workflow Details

### Workflow A: Verification Repository (FR-5.2, FR-5.3)

- Choose location: e.g. `docs/verification/`, static site, API, or dedicated repo
- Define format: pass/fail per step, attestation summary
- Associate each CVM (app-id, custom domain, or deployment id) with its verification output
- Ensure structure supports multiple CVMs (lit-api-server, lit-api-server-next, etc.)

### Workflow B: Deploy Workflow Integration (FR-5.1, FR-5.4)

- Add verification job to deploy workflow(s): `deploy-phala.yml` (dev), plus production workflow for DeRoT/self-hosted when available
- **Production**: Ensure CVM is created with Onchain KMS on Base (Phala dashboard "Onchain KMS" when creating app; DstackApp contract from Phase 1)
- Verification SHALL run after CVM is live; base_url = gateway URL (attestation at `/.dstack/`; gateway is required ingress)
- Wait for attestation availability (reuse or extend wait-for-api pattern)
- Run `cargo run -p verify-cvm` with base_url = gateway URL; verifier checks DstackApp/DstackKms (Onchain KMS)
- Capture verifier output; publish to repository from Workflow A

## Integration

- Workflow B consumes repository format from A
- Both can start once repository format is defined
- Workflow B needs: base_url (gateway URL; attestation at `/.dstack/`), app_compose or `--expected-compose-hash`
- Production: base_url = gateway URL (custom domain or deployment); gateway `/.dstack/` assumed; **Onchain KMS on Base** (app created with DstackApp)

## Files to Create/Modify

| File | Action | Workflow |
|------|--------|----------|
| Verification repository (location TBD) | Create/configure | A |
| `.github/workflows/deploy-phala.yml` | Modify — add verify + publish job (dev) | B |
| Production deploy workflow (DeRoT/self-hosted) | Add verify + publish when available | B |

## Exit Criteria / Gate

- Deploy triggers verification automatically
- Verification results published to repository
- Each CVM identifiable with its output

## Next Phase

→ [PLAN-phase-4.md](PLAN-phase-4.md) (Documentation)
