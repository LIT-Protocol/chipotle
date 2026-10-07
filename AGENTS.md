# Repository Agent Context (Root)

## Purpose
Monorepo root for Chipotle — the Lit Protocol management API and its surrounding
services. This file holds repo-wide conventions; each subfolder has its own
`AGENTS.md` describing what lives there. Start here, then read the folder-level file
before modifying code.

## Project Overview
This is a Web3/crypto monorepo. It features a Rust backend, Solidity smart contracts,
and a JavaScript/TypeScript frontend / monorepo tooling layer.

## Monorepo Architecture
- Tooling: there is no repo-root Cargo workspace — build each Rust crate from its own
  `Cargo.toml`. Some crates are themselves multi-crate workspaces (e.g. `lit-actions`,
  `lit-core`), so run `cargo` from that crate's directory. The `e2e` suite uses pnpm.
- Language Boundaries:
  - `lit-actions`, `lit-api-server`, `lit-billing-core`, `lit-payments`,
    `lit-triggers`, `lit-core`: Rust (services & indexers)
  - `lit-static`: JavaScript (static dapps, SDK bundles, contract ABIs)
  - `e2e`: TypeScript / Playwright (end-to-end tests) — see `e2e/AGENTS.md`
  - `otel-collector`: OpenTelemetry collector config

## Global Constraints
- CRITICAL: Never mix package commands. Use each language's own tool — `cargo`
  (per-crate `--manifest-path`) for Rust, `pnpm` for the `e2e` suite. There is no
  repo-root cargo workspace, so build each crate from its own `Cargo.toml`.
- Always check for a folder-level `AGENTS.md` inside subdirectories before modifying
  code there.
- Language boundaries are strict. Do not introduce Rust dependencies into JS tools, or
  vice versa, without human approval.

## Private-first development and public releases

- **All development and security fixes must be proposed and merged only in
  `LIT-Protocol/chipotle-private`.** The public `LIT-Protocol/chipotle` repository
  is a release destination, not a destination for individual fix PRs.
- Check the remote URL before every push or PR operation. A workspace's `origin`
  may point to the public repository even when the task belongs in private. Use
  an explicit private repository URL / `gh --repo LIT-Protocol/chipotle-private`.
- When the repositories diverge, bring public `main` into private through a
  dedicated private sync PR. Preserve private-only fixes, merge the sync PR
  first, then retarget dependent private PRs to private `main` and merge them.
- Do not push development/security branches, individual fix commits, detailed
  PRs, test logs, or review reports to the public repository. Authorization to
  work on a fix or sync into private does **not** authorize public publication.
- At release time, only with explicit user authorization for that release,
  prepare one bundled private-to-public release PR. Sanitize its title,
  description, branch name, and public commit messages. Keep vulnerability
  details, exploit prerequisites, reproduction steps, security regression-test
  explanations, private issue links, and deployment gaps out of public prose.
  Keep detailed review and validation records in the private repository.
- Coordinate publication and deployment timing with the release owner. A vague
  PR description does not hide its diff, commits, or CI output; do not publish
  the release branch early. Never infer a public release from a private merge.
- If sensitive material is accidentally published, notify the user promptly.
  Closing a PR, editing its body, and deleting its branch do not permanently
  remove GitHub's retained diff, edit history, or cached commits. Repository
  administrators can archive a PR to hide it from public view; permanent
  sensitive-data removal requires GitHub Support. Do not claim deletion until
  it has been verified.
