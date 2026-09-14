# Rename `next` → `test` and Redeploy From Scratch

**Status:** Draft for review
**Author:** brendon@litprotocol.com
**Date:** 2026-09-14

## Decisions locked in (from review)

| Decision | Choice |
|---|---|
| Environment name | `next` → **`test`** |
| Public domain | **`test.chipotle.litprotocol.com` — UNCHANGED.** The existing domain already matches the new name, so there is no DNS/cert cutover and no client/dashboard/k6/docs domain churn. |
| Phala TEE app | **Reuse the existing `app_id` / KMS identity** (keep the DstackApp AppAuth contract + `PHALA_DSTACKAPP_PRIVATE_KEY`), but tear down the old CVM instances and provision fresh ones renamed to `test`. Derived keys / PKP addresses stay stable. |
| Account-config contract (Base) | **Deploy a fresh test contract** and point `NodeConfig.test.toml` at it. Existing accounts / API keys / PKP registrations are dropped. |
| Migration strategy | **Tear down `next` first, then rebuild as `test`.** Accepts a short down window (this environment only — prod untouched). |

> **Big win of naming it `test`:** the domain is already `test.chipotle.litprotocol.com`. Compared with a `staging.*` rename, this eliminates the new-cert issuance, the Route 53 CNAME/TXT cutover, and every hardcoded-domain edit in k6, the dashboard, and docs. The work collapses to renaming **internal identifiers** only.
>
> **Reconciliation note:** "reuse app_id" + "tear down first" are compatible. The `app_id` is an on-chain contract identity, not a running VM. We keep that contract + its private key and re-allowlist device IDs; only the CVM *instances*, GCP project, Cloudflare project, and account-config contract are rebuilt. The domain stays put.

---

## 1. What "next" is today (current-state inventory)

The `next` environment IS this deployment. It is made of:

| Component | Current value |
|---|---|
| Phala app name | `chipotle-next` (app_id `0x969a8c14…`) |
| Phala CVM instances | `chipotle-next-rep-sa6xj` (prod2 node); a second ping-pong instance (prod4) referenced via `vars.NEXT_CVM_A` / `vars.NEXT_CVM_B` |
| Public domain | `test.chipotle.litprotocol.com` (TLS via dstack-ingress + Let's Encrypt DNS-01 on Route 53) |
| GCP project (telemetry) | `chipotle-next` |
| Account-config contract (Base) | `0x98e501fab2d60a5119a185e1563f10cb54bc6068` (in `NodeConfig.next.toml`) |
| Node config file | `lit-api-server/NodeConfig.next.toml` |
| Stripe | sandbox/test keys (`STRIPE_SANDBOX_*` secrets) |
| Dashboard (Cloudflare Pages) | project `lit-static-next` (and `lit-static-dev` from `main`), API base injected as `test.chipotle.litprotocol.com` |
| Deploy trigger | push to `main` → `deploy-staging.yml` (zero-downtime ping-pong) |
| k6 test data | `k6/data/accounts.next.json`, baseline `k6/baselines/soak.next.json` |

**Naming is currently muddled** — worth cleaning up as part of "make this concise":
- `main` branch deploys to `chipotle-next` (per CI), but `justfile` still maps `main → chipotle-dev`. Contradiction.
- A separate, deprecated `chipotle-dev` app + `dev.chipotle.litprotocol.com` + `lit-static-dev` Cloudflare project still exist.
- `deploy-static.yml` still lists a `next` git branch trigger that is otherwise retired.
- The workflow file is called `deploy-staging.yml` even though nothing else uses the word "staging" — consider renaming to `deploy-test.yml` for consistency (optional).

---

## 2. Naming map (old → new)

| Kind | Old | New |
|---|---|---|
| Phala app | `chipotle-next` | `chipotle-test` |
| Phala CVM instances | `chipotle-next-rep-*` | `chipotle-test-rep-*` |
| Public domain | `test.chipotle.litprotocol.com` | **unchanged** |
| GCP project | `chipotle-next` | `chipotle-test` (new project — GCP project IDs are immutable) |
| Account-config contract | `0x98e5…` | **new** (deploy fresh) |
| NodeConfig file | `NodeConfig.next.toml` | `NodeConfig.test.toml` |
| Cloudflare Pages project | `lit-static-next` | `lit-static-test` |
| GitHub Actions vars | `NEXT_CVM_A`, `NEXT_CVM_B` | `TEST_CVM_A`, `TEST_CVM_B` |
| k6 baseline | `k6/baselines/soak.next.json` | `k6/baselines/soak.test.json` |
| k6 accounts data | `k6/data/accounts.next.json` | `k6/data/accounts.test.json` |
| k6 NETWORK selector | `NETWORK=next` | `NETWORK=test` |

---

## 3. Infrastructure work (out-of-repo, one-time)

Do these before/alongside the code PR. **Order matters** given "tear down first".

### 3.1 Preserve identity
- [ ] Record the existing `chipotle-next` `app_id` and its `PHALA_DSTACKAPP_PRIVATE_KEY` — this is the identity we keep.
- [ ] Record the AppAuth/DstackApp contract address and current device allowlist.

### 3.2 Tear down old `next`
- [ ] `phala cvms stop` then `delete` both `chipotle-next` CVM instances.
- [ ] **Leave the `test.chipotle.litprotocol.com` DNS in place** — the rebuilt env reclaims the same domain. (The new CVM's dstack-ingress re-writes the ALIAS/TXT on boot; existing CAA/zone settings are reused.)
- [ ] Delete Cloudflare Pages project `lit-static-next`.
- [ ] Decide GCP project `chipotle-next`: keep for historical metrics, or delete. (New project is created regardless.)

### 3.3 Provision new `test`
- [ ] Create GCP project `chipotle-test`; enable Cloud Monitoring/Trace/Logging; mint a service-account JSON (→ `GCP_SERVICE_ACCOUNT_JSON` secret; project id → deploy input).
- [ ] Deploy a fresh **account-config contract** on Base (`just contracts-deploy` path / `lit_node_express` deploy task); capture its address for `NodeConfig.test.toml`.
- [ ] Provision **two** `chipotle-test` CVMs on two Phala nodes under the **reused `app_id`** (`just deploy-new chipotle-test` for the first; replicate for the second). Ping-pong requires two.
- [ ] **Allowlist both nodes' device IDs** on the AppAuth contract (`manual_phala-add-device.yml`) — KMS won't release keys otherwise.
- [ ] First boot re-issues the LE cert for `test.chipotle.litprotocol.com` (one cert — well under the 5/week/hostname limit; the `chipotle` zone has no CAA `accounturi` lock).
- [ ] Create Cloudflare Pages project `lit-static-test`.

### 3.4 GitHub repo settings
- [ ] Rename Actions **vars** `NEXT_CVM_A`/`NEXT_CVM_B` → `TEST_CVM_A`/`TEST_CVM_B` (set to the new instance names).
- [ ] Point `GCP_SERVICE_ACCOUNT_JSON` (this env's usage) at the new project's SA if that secret is environment-scoped; otherwise confirm the deploy passes `GCP_PROJECT_ID=chipotle-test`.
- [ ] Update `LIT_PAYMENTS_STAGING_URL` if the payments backend for this env changes (name kept for now unless you want it renamed too — see Open Questions).
- [ ] Secrets `STRIPE_SANDBOX_*`, `PHALA_DSTACKAPP_PRIVATE_KEY`, `CERTBOT_AWS_*`, `BASE_CHAIN_RPC`, `PHALA_CLOUD_API_KEY`, `DOCKERHUB_TOKEN`, `LIT_INTERNAL_SHARED_SECRET`: unchanged (reused).

---

## 4. Code / config changes (the PR)

Because the domain is unchanged, this is a pure **identifier rename** — no domain-string edits in k6, the dashboard base URL, or docs.

### 4.1 Build & deploy config
- `justfile:12–16` — replace branch→env logic so app/gcp/node-config resolve to the `test` names (`chipotle-test`, `NodeConfig.test.toml`). `domain := 'test.chipotle.litprotocol.com'` on line 17 is **already correct** — no change. **Also fix the `main → chipotle-dev` contradiction** so local `just` matches CI (`main → chipotle-test`).
- `justfile.sim:176,181` — `NodeConfig.next.toml` → `NodeConfig.test.toml`.
- `justfile.deploy:85` — comment `chipotle-next` → `chipotle-test`.
- `Dockerfile:51–52` and `Dockerfile.lit-api-server:30–31` — `ARG NODE_CONFIG=NodeConfig.next.toml` → `NodeConfig.test.toml` (+ comments).

### 4.2 Node config
- `git mv lit-api-server/NodeConfig.next.toml lit-api-server/NodeConfig.test.toml`; update `contract_address` to the freshly-deployed test contract.

### 4.3 CI workflows (`.github/workflows/`)
- `deploy-staging.yml` — `NEXT_CVM_A/B` → `TEST_CVM_A/B`; `chipotle-next-rep-sa6xj` → new default instance; `gcp_project_id=chipotle-next` → `chipotle-test`; `node_config=NodeConfig.next.toml` → `NodeConfig.test.toml`; update `chipotle-next` comments; `baseline_file: ../baselines/soak.next.json` → `soak.test.json`. `DOMAIN="test.chipotle.litprotocol.com"` stays. *(Optional: rename the file to `deploy-test.yml`.)*
- `deploy-static.yml` — project `lit-static-next` → `lit-static-test`. The injected API URL (`test.chipotle…`) is **unchanged**. Drop/rename the retired `next` git-branch trigger and `deploy-next` job as part of concise cleanup.
- `k6-loadtest.yml`, `k6-smoke.yml`, `k6-correctness.yml` — update the `chipotle-next` comments (the domain in them is unchanged).
- `manual_phala-envs-update.yml:47` — default `chipotle-next` → `chipotle-test` (domain default on line 42 unchanged).
- `manual_contract-upgrade.yml:8` — comment `next → NodeConfig.next.toml` → `test → NodeConfig.test.toml`.
- `phala-simulator.yml:202` — `NodeConfig.next.toml` → `NodeConfig.test.toml`.
- `manual_phala-add-device.yml:8`, `manual_phala-propose-add-device.yml:9` — `chipotle-next` → `chipotle-test`.
- **Prod workflows are untouched** (`deploy-prod-*`, `manual_contract-upgrade-prod-*`, `manual_k6-prod-test.yml`, `deploy-prod-3-static.yml`).

### 4.4 k6
- `git mv k6/baselines/soak.next.json k6/baselines/soak.test.json`; inside, `"env": "next"` → `"test"` and the note text (base_url unchanged).
- `git mv k6/data/accounts.next.json k6/data/accounts.test.json` (or regenerate via `seed-accounts` after the fresh contract is live — data is invalid once the account-config contract changes).
- `k6/justfile:19,22,23–24` — rename the `_base_url_next`/`_accounts_file_next`/`NETWORK=next` selectors to `test` (the base-URL *value* stays `test.chipotle…`). **Careful:** `_base_url_main` and `_base_url_next` both already point at this domain — you're only renaming the selector key, not the URL.
- No domain edits needed in `k6/defaults.ts`, `k6/update-soak-baseline.sh`, `k6/loadtest/{spike,soak,breakpoint}.spec.ts`, or the k6 READMEs — they already point at `test.chipotle…`.

### 4.5 Dashboard / static (`lit-static/`)
- `lit-static/dapps/monitor/index.html:490` — dropdown option **label** `Next - Phala` → `Test - Phala` (the URL is already `test.chipotle…`).
- `lit-static/account_config_full_abi.js:369–370` — comment `next (staging)` → `test`, and update the contract-address map entry to the **new contract address**.
- (`auth.js` needs no change — API base is injected at deploy time and stays `test.chipotle…`.)

### 4.6 Docs
- `docs-reserve/lit-actions/languages.mdx:8,72,86` — user-facing: rename the `next` environment references to `test`.
- `architectureDocs/deployment/secrets-management.md:52` — `chipotle-next` → `chipotle-test`.
- `architectureDocs/deployment/planning/PLAN-phase-3.md:29` and `docs/deployment/planning/PLAN-phase-3.md:29` — `lit-api-server-next` → `-test`.
- `plans/zero-downtime-pingpong-deploy.md:17,40,44,162,188` — update the `next`/`chipotle-next` names (domain rows stay `test.chipotle…`). `plans/k6-perf-regression-gate.md:65` — `soak.next.json` → `soak.test.json`.
- Optional: note the change in `CHANGELOG.md`.

### 4.7 No changes needed
- `examples/` (all point at prod). `examples/lit-solver-vault/dashboard/next.config.js` is a **Next.js framework file** — do not touch.
- `e2e/` (localhost defaults, env-overridable).
- `README.md` and the rest of `docs/` (prod domains only).

---

## 5. Deploy & verify
1. Merge the rename PR to `main` → triggers the deploy workflow onto the new `chipotle-test` CVMs.
2. Pipeline gates run automatically: `wait-for-api-available` → `verify-attestation` → `openapi-spec-check` → `k6-smoke` → `k6-correctness` → `k6-loadtest (soak)` → confirm-cutover (reclaims `test.chipotle…`).
3. Re-seed k6 accounts against the new contract (`just -f k6/justfile NETWORK=test seed-accounts`) and refresh the soak baseline (`k6/update-soak-baseline.sh`).
4. Deploy dashboard to `lit-static-test`; smoke-test login against `test.chipotle.litprotocol.com`.
5. Verify TLS cert re-issued for the domain; verify telemetry lands in the `chipotle-test` GCP project.

---

## 6. Risks & watch-outs
- **Down window:** tear-down-first means this env is offline during rebuild. Prod is unaffected throughout.
- **Data loss (intended):** fresh account-config contract drops all accounts/API keys/PKP registrations for this env. k6 account data must be regenerated.
- **`test` is an overloaded word:** watch for confusion with `just test` (k6 recipes), `test.sh`, and Rust test fixtures. `NodeConfig.test.toml` is fine but review any tooling that globs `*test*`.
- **Let's Encrypt limit:** one new cert on first boot (same hostname) — fine. Keep the two `chipotle-test` CVMs long-lived so in-place upgrades reuse the `cert-data` volume.
- **Device allowlist:** both nodes' `device_id`s MUST be allowlisted on the AppAuth contract or KMS won't release keys — the classic ping-pong failure mode.
- **Ping-pong needs two instances:** provisioning only one silently falls back to legacy single-CVM in-place deploys (with downtime per deploy).
- **GCP project IDs are immutable** — this is a *new* project, not a rename; update every `GCP_PROJECT_ID` reference and the SA key.

---

## 7. Open questions to confirm before executing
1. **Deprecated `chipotle-dev` / `dev.chipotle.litprotocol.com` / `lit-static-dev`:** fold into this cleanup (retire), or leave alone? (Recommend retiring to make naming concise.)
2. **`main` as the deploy branch:** confirm `main` stays the deploy branch (so `justfile` maps `main → chipotle-test`), and that the retired `next` git-branch triggers can be removed from `deploy-static.yml`.
3. **Rename `deploy-staging.yml` → `deploy-test.yml`** and the `LIT_PAYMENTS_STAGING_URL` var → `LIT_PAYMENTS_TEST_URL` for full consistency, or leave the "staging" labels on those two to minimize churn?
4. **GCP old project:** delete `chipotle-next` or retain for historical metrics?
5. **lit-payments backend:** does the payments URL need to change for the rebuilt env?
