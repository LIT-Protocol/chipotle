# Changelog

User-facing changes to the Chipotle API and stack. For the full history, see
`git log` and the [release tags](https://github.com/LIT-Protocol/chipotle/tags).
Release verification (image digests, attestation, governance) is covered in the
[upgrade governance docs](https://developer.litprotocol.com/architecture/verification/upgrade-governance).

## Unreleased

### Changed
- **All API error responses are now JSON** (`{error, message, fix, docs_url}`)
  instead of HTML error pages. See the
  [Errors reference](https://developer.litprotocol.com/management/errors).
- **Invalid API keys now return `401 Unauthorized`** (previously `402` on
  billed endpoints and `400`/`500` on billing endpoints). `402 Payment
  Required` is reserved for real accounts with insufficient credits, and its
  body now states the amount needed, your balance, and how to fund.
- **Billing outages now return `503`** instead of `402`.
- **Failed requests are no longer charged.** The flat $0.01 management charge
  settles only after the operation succeeds.
- `POST /create_wallet` added; `GET /create_wallet` still works but is
  deprecated (metered writes don't belong on GET).

### Added
- Optional starter credits for new accounts (`STARTER_CREDITS_CENTS`, default
  off): lets the quickstart run before adding funds.
- `CONTRIBUTING.md`, expanded `.env.example`, and per-component READMEs.

### Fixed
- Release builds no longer report a spurious `-modified` version suffix
  (`.dockerignore` excluded a tracked file, dirtying `git describe` inside the
  image build).

## Historical releases

| Tag | Date |
|--------|------------|
| v1.1.7 | 2026-06-02 |
| v1.1.6 | 2026-05-31 |
| v1.1.4 | 2026-05-29 |
| v1.1.3 | 2026-05-29 |
| v1.1.2 | 2026-05-11 |
| v1.1.1 | 2026-05-06 |
| v1.1.0 | 2026-05-06 |
| v1.0.x | 2026-03-29 → 2026-04-20 |
| v0.1–v0.2 | 2026-03-13 → 2026-03-17 |

(Tags predating this file aren't annotated here — `git log <tag>` has the detail.)
