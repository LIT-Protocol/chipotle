# Security Policy

Lit Chipotle runs inside Intel TDX Trusted Execution Environments with key release
gated by smart contracts on Base. We take the security of the stack — code,
contracts, deployment pipeline, and governance — seriously, and we welcome reports
from the security community.

## Reporting a vulnerability

**Please do not open a public GitHub issue for security vulnerabilities.**

Report privately, via either:

- **Email:** [security@litprotocol.com](mailto:security@litprotocol.com) (preferred).
  Encrypt sensitive details with our PGP key if you have one; ask in your first
  message and we will provide it.
- **GitHub:** open a [private security advisory](https://github.com/LIT-Protocol/chipotle/security/advisories/new)
  on this repository.

In your report, please include:

- A description of the issue and its potential impact.
- Steps to reproduce, or a proof of concept.
- The component affected (API server, Lit Actions runtime, contracts, deployment/CI,
  governance, attestation/verification flow).
- Any relevant version, commit hash, compose hash, or deployment URL.

## What to expect

- **Acknowledgement** within 3 business days.
- **Triage and severity assessment** within 7 business days, with an initial
  remediation plan.
- **Coordinated disclosure.** We will work with you on a disclosure timeline and
  credit you (if you wish) once a fix is released. Please give us a reasonable window
  to remediate before any public disclosure.

## Scope

In scope:

- The `lit-api-server`, `lit-actions` runtime, and `lit-static` dashboard.
- Smart contracts in this repository (account/permission model, and the
  attestation-governance contracts on Base).
- The deployment pipeline and TEE attestation / verification flow.
- The on-chain governance and key-release model.

Out of scope (report upstream, but tell us if it affects us):

- Vulnerabilities in Intel TDX, the dstack OS, or Phala Cloud infrastructure —
  report to [Intel](https://www.intel.com/content/www/us/en/security-center/default.html),
  [dstack](https://github.com/Dstack-TEE/dstack), or
  [Phala](https://docs.phala.com/) respectively.
- Third-party dependencies — report upstream; we track advisories via `deny.toml`.

## Verifying the production deployment

Much of our security model is publicly verifiable rather than asserted. Before
reporting an issue that depends on "what code is running," confirm the live state
yourself:

- [What Is Attestation?](docs/architecture/verification/attestation.mdx) — the model in plain English.
- [Upgrade Governance](docs/architecture/verification/upgrade-governance.mdx) — how releases are approved (2-of-4 Safe on Base).
- [On-Chain KMS](docs/architecture/verification/onchain-kms.mdx) — how key release is gated.
- [Verify in 30 Seconds](docs/architecture/verification/quick-verify.mdx) — Trust Center report + three terminal commands.

## Operational security references (internal)

- [Upgrade governance & release approval](architectureDocs/deployment/vm-code-upgrade.md)
- [Incident response & key-compromise runbook](architectureDocs/deployment/incident-response.md)
- [Secrets management](architectureDocs/deployment/secrets-management.md)
