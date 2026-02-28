# One-Time CVM Verification — Requirements

When a Phala CVM is booted, **automated verification** (company-run, post-deployment) uses the verifier tool and publishes results. End users MAY optionally run the verifier themselves; they may also trust the published results. After verification, the [Complete Chain of Trust](https://docs.phala.com/phala-cloud/attestation/chain-of-trust) (Application, Platform, Network, and Governance layers) is established, and the TLS certificate serves as a **trust anchor** into that verified chain. On subsequent connections, validating that the presented certificate matches the attested one (e.g. via certificate pinning) is sufficient to establish that the connection terminates in the verified CVM.

**A clean bill is NOT required for each individual API invocation.** Verification runs once (at deployment); the TLS certificate provides the trust anchor for subsequent requests.

**Custom domain as trust anchor**: The system MUST support custom domains (e.g. the company domain) as the end-user target for connecting to cloud-hosted Lit API server CVMs. The custom domain MUST be the trust anchor and MUST NOT be outside the trust chain. Users connect to the custom domain; that domain's TLS certificate (TEE-controlled) is what they verify and pin. A redirect from custom domain to the Phala gateway domain, where TLS terminates elsewhere, would place the custom domain outside the chain and is NOT acceptable.

## Functional Requirements

### FR-1: Verification Capability

The system SHALL expose verification capability to the user. A `GET /phala/v1/verify` endpoint is one option; there may be other or better options (e.g. Trust Center, gateway attestation endpoints, on-chain records).

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-1.1 | The system SHALL make attestation data (quote, event_log, vm_config) available for CVM verification. The mechanism is implementation-defined (e.g. API endpoint, Trust Center, Phala gateway `/.dstack/` endpoints, or other). | Must |
| FR-1.2 | The system SHALL make app_compose or compose-hash available for code authentication, or document a fallback (e.g. `--expected-compose-hash` for offline verification). | Should |
| FR-1.3 | Verifiers SHALL be able to perform verification when attestation data is obtained via the chosen mechanism. | Must |

### FR-2: KMS / Root of Trust Configurability

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-2.1 | The KMS/Root of Trust SHALL be configurable at compile time via a selector (e.g. Cargo feature flags). | Must |
| FR-2.2 | The system SHALL support Phala Cloud as one RoT option (e.g. `pcloud` feature). | Must |
| FR-2.3 | The system SHALL support DeRoT on Base as another RoT option (e.g. `derot` feature). | Must |
| FR-2.4 | The selector SHALL enable local/internal testing of deployed networks by building with the appropriate RoT backend. | Must |
| FR-2.5 | The system SHALL be testable locally using the Phala-provided dstack simulator (e.g. via `DSTACK_SIMULATOR_ENDPOINT` or equivalent). | Must |
| FR-2.6 | Production SHALL use Onchain KMS on Base ([Cloud vs Onchain KMS](https://docs.phala.com/phala-cloud/key-management/cloud-vs-onchain-kms)); DstackApp contract deployment and app creation configured for blockchain-enforced governance. | Must |

### FR-3: Custom Domain Support

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-3.1 | The system SHALL support custom domains (e.g. company domain) as the end-user target for connecting to cloud-hosted Lit API server CVMs. | Must |
| FR-3.2 | The custom domain SHALL be the trust anchor; it SHALL NOT be outside the trust chain. | Must |
| FR-3.3 | TLS SHALL terminate at the custom domain with a TEE-controlled certificate, such that users connect to and verify the custom domain directly. | Must |
| FR-3.4 | The custom domain's certificate SHALL be the one attested in the verification chain and used for certificate pinning on subsequent connections. | Must |

### FR-4: Complete Chain of Trust Verifier Tool

The system SHALL provide a verifier tool that performs **all** steps listed as sufficient in the [Phala Complete Chain of Trust](https://docs.phala.com/phala-cloud/attestation/chain-of-trust) checklist. All items in that checklist must pass for complete trustless verification.

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-4.1 | A standalone verifier tool SHALL accept attestation data via base_url (to fetch from whatever mechanism exposes verification capability), or directly: quote (hex), event_log, optionally app_compose or `--expected-compose-hash`. | Must |
| FR-4.2 | The verifier SHALL implement ALL steps from the [Complete Verification Checklist](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#complete-verification-checklist): Application Layer, Platform Layer, Network Layer (custom domains), and Governance Layer (where applicable). | Must |
| FR-4.3 | The verifier SHALL support `--skip-*` flags for partial verification when expected values or on-chain governance are unavailable (e.g. optional items). | Should |
| FR-4.4 | The verifier SHALL link to Trust Center (`https://trust.phala.com/app/{app-id}`) in output. | Should |
| FR-4.5 | The verifier SHALL be designed for one-time use: user runs once before trusting the CVM, then sends API requests. | Must |

### FR-5: Company Verification Repository

The company SHALL maintain an information repository with company-run verification outputs for each CVM. Automated verification uses the verifier tool (FR-4) and runs post-deployment. End users MAY optionally run the verifier themselves; they may also trust the published results.

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-5.1 | The company SHALL run automated verification of each new CVM after deployment using the verifier tool (FR-4). | Must |
| FR-5.2 | The company SHALL publish verification results (pass/fail per step, attestation summary) to a company-maintained information repository. | Must |
| FR-5.3 | The repository SHALL associate each CVM (e.g. by app-id, custom domain, or deployment identifier) with its verification output. | Must |
| FR-5.4 | The deploy workflow SHALL trigger verification and publishing as part of post-deployment automation. | Must |

## Verification Requirements (Complete Chain of Trust)

The verifier SHALL implement all items from the [Phala Complete Verification Checklist](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#complete-verification-checklist). Items marked optional in the Phala doc MAY be skipped when `--skip-*` flags are used or when governance is not in use.

Requirements marked **Source: [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof)** are derived from the Phala Security Proof, which proves by contradiction that each verification item is necessary—any modification to the TCB causes that specific check to fail.

### VR-1: Application Layer (What You Control)

| ID | Requirement | Priority | Source |
|----|-------------|----------|--------|
| VR-1.1 | The verifier SHALL verify reportData contains expected challenge or public key (prevents replay attacks). | Must | |
| VR-1.2 | The verifier SHALL extract compose-hash from RTMR3 event log and assert it matches calculated hash from app_compose. | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| VR-1.3 | The verifier SHALL verify all Docker images in app-compose use SHA256 digests (not mutable tags). | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| VR-1.4 | The verifier SHALL perform RTMR3 event log replay and assert replayed RTMR3 matches quoted RTMR3. | Must | |
| VR-1.5 | The verifier SHALL verify image digests link to audited source code (Sigstore or reproducible builds). | Must | |
| VR-1.6 | The verifier SHALL verify compose-hash is whitelisted in DstackApp contract when governance is used. | Must (if governance) | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |

### VR-2: Platform Layer (Infrastructure)

| ID | Requirement | Priority | Source |
|----|-------------|----------|--------|
| VR-2.1 | The verifier SHALL verify TDX quote signature is valid (Intel root certificates). | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| VR-2.2 | The verifier SHALL verify `tee_tcb_svn` matches latest security patches. | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| VR-2.3 | The verifier SHALL compare MRTD and RTMR0-2 from the quote with calculated OS measurements. | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| VR-2.4 | The verifier SHALL verify VM config (CPU, memory, GPU) matches deployment. | Must | |
| VR-2.5 | The verifier SHALL verify OS image hash is whitelisted in DstackKms contract. | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| VR-2.6 | The verifier MAY verify OS rebuilt reproducibly from meta-dstack source. | Could | |
| VR-2.7 | The verifier SHALL extract KMS ID from `key-provider` event and verify it is known and trusted. | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| VR-2.8 | The verifier SHALL verify the KMS's own attestation quote is valid. | Must | |
| VR-2.9 | The verifier SHALL verify KMS aggregated MR is whitelisted in DstackKms contract. | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |

### VR-3: Network Layer (Custom Domains)

The TLS certificate verified SHALL be the custom domain's certificate—the end-user target. This certificate is the trust anchor and MUST be inside the trust chain.

| ID | Requirement | Priority | Source |
|----|-------------|----------|--------|
| VR-3.1 | The verifier SHALL verify TLS certificate fingerprint matches served certificate. | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| VR-3.2 | The verifier SHALL verify evidence files at `/evidences/` cryptographically bind to quote (reportData). | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| VR-3.3 | The verifier SHALL verify CAA DNS records restrict certificate issuance to TEE-controlled CA. | Must | |
| VR-3.4 | The verifier SHALL validate the custom domain's TLS certificate chain to a trusted CA. | Must | |
| VR-3.5 | The verifier MAY verify the certificate is logged in Certificate Transparency logs. | Could | |

### VR-4: Governance Layer

| ID | Requirement | Priority | Source |
|----|-------------|----------|--------|
| VR-4.1 | The verifier SHALL verify smart contract addresses are trusted. | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| VR-4.2 | The verifier SHALL verify contract permissions match security policy. | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| VR-4.3 | The verifier SHALL document or verify contract ownership and upgrade mechanisms. | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |

## Deployment Requirements

### DR-1: Docker Image Pinning

| ID | Requirement | Priority | Source |
|----|-------------|----------|--------|
| DR-1.1 | Deployment SHALL use `@sha256:` digests in docker-compose for verifiable code authentication. | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |
| DR-1.2 | The deploy workflow SHALL pin images by digest (not mutable tags like `latest`). | Must | [Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) |

### DR-2: Documentation

| ID | Requirement | Priority |
|----|-------------|----------|
| DR-2.1 | Deployment docs SHALL describe the one-time CVM verification trust model. | Must |
| DR-2.2 | Deployment docs SHALL document the Complete Chain of Trust verification steps and how to run the verifier tool. | Must |
| DR-2.3 | Deployment docs SHALL reference Trust Center and Phala attestation endpoints. | Must |
| DR-2.4 | Deployment docs SHALL stay in sync with Orchestration (Development vs Released) and DeRoT sections. | Must |
| DR-2.5 | Deployment docs SHALL document custom domain setup such that the company domain is the trust anchor (TLS passthrough / dstack-ingress; no redirect pattern that places the custom domain outside the chain). | Must |
| DR-2.6 | Deployment docs SHALL document the company verification repository: how to access it, how CVM verification outputs are published, and how to interpret results. | Must |

## Non-Functional Requirements

### NFR-1: Trust Model

| ID | Requirement | Priority |
|----|-------------|----------|
| NFR-1.1 | Verification SHALL be performed using the verifier tool. Automated verification (company-run, post-deployment) is the primary path; end users MAY optionally run the verifier themselves. | Must |
| NFR-1.2 | After full verification (including TLS certificate), the TLS certificate SHALL serve as a trust anchor into the verified chain. On subsequent connections, validating that the presented certificate matches the attested one (e.g. certificate pinning) is sufficient to establish trust. | Must |
| NFR-1.3 | Verification SHALL be performed outside the CVM; only external verification provides security guarantees. | Must |

**Claim validation**: Phala Trust Center documents that for gateway deployments, the TLS certificate's private key is generated entirely within the TEE. Full verification proves the cert is bound to the attested TEE, code, and OS. Therefore the cert is a valid trust anchor: subsequent connections presenting the same attested cert can be trusted to terminate in the verified CVM. The user must still validate the presented cert on each connection (e.g. pin the attested public key); this is not "implicit" trust but trust anchored in the verified certificate. The custom domain MUST be the trust anchor—users connect to the company domain; that domain's TEE-controlled cert is verified and pinned. The custom domain cannot be a redirect to another domain where TLS terminates; it must be inside the trust chain.

### NFR-2: Vendor Independence (Production)

| ID | Requirement | Priority |
|----|-------------|----------|
| NFR-2.1 | Production deployments MAY depend on open dstack components; they SHALL NOT depend on Phala-exclusive infrastructure or tooling. | Must |
| NFR-2.2 | Phala Cloud MAY be used for development. Production SHALL be deployable on any dstack-compatible platform (e.g. DeRoT, self-hosted). | Must |

### NFR-3: Out of Scope

| ID | Requirement | Priority |
|----|-------------|----------|
| NFR-3.1 | Per-request attestation or clean bill on each API invocation is NOT in scope. | N/A |
| NFR-3.2 | Payment/billing integration (LITKEY, etc.) is NOT in scope. | N/A |

### NFR-4: Future Considerations

The current design (automated verification with optional end-user verification) is appropriate for this first version given the system runs on a single machine.

| ID | Future Consideration | Status |
|----|----------------------|--------|
| NFR-4.1 | The proof of cleanness will be recorded on-chain; the clean bill will not be executed by the end user directly. | Future |
| NFR-4.2 | A trust authority will attest and record verification results on-chain. The identity of this authority (DAO, company, etc.) is left for future design. | Future |
| NFR-4.3 | Users may optionally run verification themselves (self-verify) OR trust the on-chain attested record without running the verifier. | Future |

## References

- [Phala: Complete Chain of Trust](https://docs.phala.com/phala-cloud/attestation/chain-of-trust) — Full verification checklist (authoritative)
- [Phala: Security Proof](https://docs.phala.com/phala-cloud/attestation/chain-of-trust#security-proof) — Proof by contradiction that each verification item is necessary
- [Phala: Verify Your Application](https://docs.phala.com/phala-cloud/attestation/verify-your-application)
- [Phala: Trust Center Technical](https://docs.phala.com/dstack/trust-center-technical)
- [Phala: Trust Center Network Verification](https://docs.phala.com/phala-cloud/attestation/trust-center-verification)
- [dstack-examples verify.py](https://github.com/Dstack-TEE/dstack-examples/blob/main/attestation/rtmr3-based/verify.py)
- [dcap-qvl](https://github.com/Phala-Network/dcap-qvl)
- [dstack-mr](https://github.com/Dstack-TEE/dstack/tree/master/dstack-mr/cli)
- [meta-dstack](https://github.com/Dstack-TEE/meta-dstack)

## Priority Legend

| Priority | Meaning |
|----------|---------|
| Must | Required for the feature to work |
| Should | Strongly recommended; fallback acceptable |
| Could | Optional enhancement |
| N/A | Not applicable |
