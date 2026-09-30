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

Every report **must** include:

- **A working proof of concept** — a script, request sequence, Foundry/Hardhat
  test, Lit Action, or exact steps with real inputs that we can replay ourselves.
  Screenshots, video, or a description of a vulnerability class do not qualify.
- **Demonstrated security impact** — which key, account, funds, data, or guarantee
  (attestation, permission model, sandbox isolation, governance) is compromised.
- **The exact target** — endpoint, contract address and function, or file and line,
  and the component affected (API server, Lit Actions runtime, contracts,
  deployment/CI, governance, attestation/verification flow).
- **Reproduction context** — the commit hash, compose hash, or deployment URL tested.
- **One vulnerability per report**, written and verified by a human. Unverified
  scanner or AI-tool output is closed immediately; repeat submissions are banned.

Reports missing any of the above are closed without triage and are not eligible
for a reward.

## Bug bounty

We pay rewards for valid, previously unknown, **demonstrated** vulnerabilities that
we assess as Medium severity or higher. Low and Informational findings are not
rewarded. See the [Bug Bounty Program](docs/architecture/verification/bug-bounty.mdx)
page for the full report requirements, scope, exclusions (insider attacks, the
marketing website, DoS, best-practice findings without impact, and more), and the
rules of engagement.

Including your discovery methodology increases the likelihood of a reward. Share
the steps you took to find and investigate the bug, the tools and any AI prompts
you used, and how you personally verified the finding. This is encouraged, but
optional; all report requirements and reward eligibility criteria still apply.

## What to expect

For reports that meet the requirements above:

- **Acknowledgement** within 3 business days.
- **Triage and severity assessment** within 7 business days, with an initial
  remediation plan.

Reports that do not meet the requirements receive a short closure notice or no
reply.
- **Coordinated disclosure.** We will work with you on a disclosure timeline and
  credit you (if you wish) once a fix is released. Please give us a reasonable window
  to remediate before any public disclosure.

## Scope

The repository in scope is [LIT-Protocol/chipotle](https://github.com/LIT-Protocol/chipotle),
covering the following components, subject to the exclusions below:

- The `lit-api-server`, `lit-actions` runtime, and `lit-static` dashboard.
- Smart contracts in this repository (account/permission model, and the
  attestation-governance contracts on Base).
- The deployment pipeline and TEE attestation / verification flow.
- The on-chain governance and key-release model.

Out of scope:

- **User misconfiguration** — issues caused by a user configuring their setup
  insecurely, such as granting overly broad permissions or disabling available
  security controls, are user error and are not valid bounty reports. The ability
  to choose an insecure configuration is not itself a vulnerability.
- Vulnerabilities in Intel TDX, the dstack OS, or Phala Cloud infrastructure —
  report to [Intel](https://www.intel.com/content/www/us/en/security-center/default.html),
  [dstack](https://github.com/Dstack-TEE/dstack), or
  [Phala](https://docs.phala.com/) respectively, but tell us if they affect us.
- Third-party dependencies — report upstream; we track advisories via `deny.toml`.
  A working exploit of a dependency CVE against our deployment *is* in scope.
- Best-practice findings without a working exploit and demonstrated impact
  (security headers, email auth records, TLS preferences, rate limiting,
  version disclosure, scanner output). See the bug bounty page for the full list.

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
