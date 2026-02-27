# Phase 4: Documentation

**Parent**: [PLAN.md](PLAN.md) | **Requirements**: [requirements.md](requirements.md)

## Goal

Complete deployment documentation per DR-2. All six documentation requirements satisfied.

**Constraint (NFR-2)**: Document dev (Phala Cloud) and production (DeRoT, self-hosted) paths separately. Production docs emphasize open dstack; no Phala-exclusive dependencies.

## Prerequisites

- **Phases 1–3 complete** — Content exists (trust model, verifier, verification repository)

## Parallelizable Workflows

| Workflow | Tasks | Requirements | Parallelizable with |
|----------|-------|--------------|---------------------|
| **A** | Trust model, Complete Chain of Trust steps, verifier usage, attestation refs (gateway `/.dstack/`), Orchestration/DeRoT sync, **Onchain KMS on Base** | DR-2.1, DR-2.2, DR-2.3, DR-2.4 | B |
| **B** | Custom domain: design migration (TLS passthrough / dstack-ingress); document setup, current gap, verification repository | FR-3, DR-2.5, DR-2.6 | A |

## Workflow Details

### Workflow A: Core Verification Docs (DR-2.1–DR-2.4)

- **DR-2.1**: Describe one-time CVM verification trust model (verify once, TLS cert as trust anchor)
- **DR-2.2**: Document Complete Chain of Trust steps and how to run verifier (`cargo run -p verify-cvm`)
- **DR-2.3**: Reference attestation endpoints. Gateway `/.dstack/` (required ingress; assume it exists). Trust Center optional for dev.
- **DR-2.4**: Keep in sync with Orchestration (Development vs Released) and DeRoT sections. Emphasize production = DeRoT / open dstack.
- **Onchain KMS on Base**: Document DstackApp deployment, Base contract/RPC, app creation flow, governance (owner, multisig, timelock). Reference [Cloud vs Onchain KMS](https://docs.phala.com/phala-cloud/key-management/cloud-vs-onchain-kms).

### Workflow B: Custom Domain + Repository Docs (FR-3, DR-2.5, DR-2.6)

- **FR-3 (design)**: Design migration from redirect pattern to TLS passthrough / dstack-ingress. Production = our own dstack-gateway instance (DSTACK; OK for production). Document current gap (redirect is insecure). Reference CPL-5 or equivalent.
- **DR-2.5**: Document custom domain setup — company domain as trust anchor, TLS passthrough / dstack-ingress (open), no redirect pattern
- **DR-2.6**: Document company verification repository — how to access, how outputs are published, how to interpret results
- Document dev vs production deployment paths. Production: our own dstack-gateway for custom domain (DSTACK; OK for production)

## Files to Create/Modify

| File | Action | Workflow |
|------|--------|----------|
| `docs/deployment/deployment.md` | Modify — add One-Time CVM Verification section, Onchain KMS on Base setup | A |
| `docs/deployment/` | Add custom domain design doc; update deployment.md (custom domain, verification repository) | B |
| `verify-cvm/README.md` | Update if needed; include DstackApp/Base verification | A |

## Exit Criteria / Gate

- All DR-2.1–DR-2.6 requirements satisfied
- Deployment docs describe full verification flow
- Users can find and interpret verification results

## Next

No further phases. Implementation complete.
