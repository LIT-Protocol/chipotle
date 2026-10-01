# Docs Reserve

Documentation for features that are **merged to `main` but not yet in a cut
release**. developer.litprotocol.com publishes from `main` (Mintlify, content
root `docs/`), while releases are cut as tags (`v1.1.x`) — so a doc page merged
between releases goes live describing a server the public can't reach yet.
Pages in this directory are parked here, outside the Mintlify content root, so
they cannot be published (Mintlify serves pages by direct URL even when they
are absent from `docs.json` navigation — removing the nav entry alone is not
enough).

**When the next release is cut**, restore each entry below whose feature is in
the release: `git mv` the page back to its original path, re-add its
`docs.json` navigation line, and delete its entry here. Then re-run the audit
that produced this file (see "Audit method").

---

## Other post-v1.1.10 changes to revisit at the next release cut

These shipped on `main` after v1.1.10 **without** public-doc changes. When the
next release is cut, decide whether each needs docs (none are currently
described on the live site, so nothing needed reserving):

- **Permanently delete a PKP wallet** (dashboard, PR #553) — user-facing;
  `docs/management/dashboard.mdx` has no mention yet.
- **`max_get_keys_count` enforcement in key handlers** (PR #540) — a
  user-visible request limit; candidate for `docs/lit-actions/limits.mdx` or
  `docs/management/api_direct.mdx`.
- **Enterprise net-30 committed-use billing** (PR #541) — candidate for
  `docs/management/pricing.mdx` if/when offered publicly (plan:
  `plans/enterprise-committed-billing.md`).
- **In-process TEE task supervision** (PR #529) — internal.
- **Auto-fund API payer gas wallets + low-balance alerts** (PR #524) and
  **raw `get_api_payers` JSON funder allowlist** (PR #527) — operator-facing;
  self-hosting docs candidates.

## Audited docs changes that were kept live (feature already in v1.1.10)

For the record, the other `docs/` deltas between v1.1.10 and `main`
(as of 2026-07-12) describe behavior that **is** in v1.1.10 and were left live:

- **EIP-1271 smart-contract-wallet auth** (`account_modes.mdx`,
  `api_direct.mdx`, PR #526) — verification code (`ERC1271_MAGIC_VALUE`
  fallback in `lit-api-server/src/core/eip712.rs`, `lit-billing-core`) is in
  the v1.1.10 tree.
- **ChainSecured billing split** (`account_modes.mdx`, `pricing.mdx`, PR #539)
  — documents architecture-inherent behavior (sovereign admin writes never hit
  the metered endpoint), present in v1.1.10.
- **npm-package composability** (`imports.mdx`, `patterns.mdx` note,
  `index.mdx` link, PR #542) and **multiple PKPs in one action**
  (`patterns.mdx`, `secrets.mdx` link, PR #552) — both use import and
  `pkpId`-scoped APIs that exist in v1.1.10.
- **dstack deployment platform list** (`self-hosting.mdx`, PR #567) —
  informational, not tied to server code.

## Audit method

Compare the docs tree between the last release tag and `main`, then check each
delta against the tag's *code* tree:

```bash
git diff --stat $(git tag --sort=-creatordate | grep '^v' | head -1)..origin/main -- docs/
# for each documented feature: git grep <feature symbol> <tag> -- <crate>/
```

A docs delta whose underlying feature is absent from the tag's tree gets moved
here with a restore note above.
