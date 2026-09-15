# Zero-Downtime Ping-Pong Deploys on Phala (prod2 ↔ prod4)

> **Update (2026-06-15) — the `/health` healthcheck gate was dropped.** The first
> attempt at this (PR #437) added a `lit-api-server` healthcheck (+ `curl` in the
> Dockerfile) and gated the ingress on `service_healthy` so the domain only claimed
> once `/health` passed. That broke deploys (the gate prevented `dstack-ingress`
> from starting / claiming the domain) and was reverted (#493). This re-landing keeps
> **only the CI side** — `dstack-ingress` now depends on `lit-api-server:
> service_started` (claims the domain as soon as the container starts, no `/health`
> gate). The boot→ready exposure window is covered by the CI flow: verify on the cold
> box's direct URL, then `confirm-cutover`, with `rollback-on-failure` flipping the
> domain back if any post-cutover check fails. So in the sections below, treat every
> "add a healthcheck + `service_healthy` dependency" recommendation as **not done on
> purpose** — the CI verify + auto-rollback is the mitigation instead.

**Status:** Design settled (validated by a manual prod cutover 2026-06-02). Ready to automate.
**Scope:** One app, two long-lived instances on two Phala nodes. Each deploy targets the *cold* node; the new CVM's `dstack-ingress` **auto-claims** the custom domain on boot (the cutover); the old node is shut down once the new one is verified. Single `app_id` → one identity / shared keys preserved. Do `next`/staging first, then prod.

---

## 1. Goal

Today every deploy mutates the **single live CVM in place** → container restart → ~30s–2min of **downtime** while `wait-for-api-restart.yml` polls for it to return.

Instead: keep **two warm instances of the same app** on two nodes (prod2 + prod4). Each deploy:

1. Targets whichever node is **cold** (not serving).
2. Brings up the new code there and waits until it's **healthy on its own default Phala URL**.
3. Lets the cold CVM's `dstack-ingress` **auto-claim** the custom domain (rewrites Route 53 → its node's gateway). The old node keeps serving cached clients until DNS rolls over.
4. **Shuts down the old node** once the new one is confirmed serving.

Next deploy flips direction. Both nodes run the **same `app_id`** (shared KMS identity / derived keys), so the node's on-chain identity and signing keys are continuous across cutovers.

---

## 2. Current state

| Thing | Today |
|---|---|
| `next` app | `chipotle-next` — instance `chipotle-next-rep-sa6xj` (prod2, `app_id 0x969a8c14…`) |
| `dev` app | `chipotle-dev` (prod5, `app_id f8fce543…`) |
| prod app | `chipotle-prod` — instance `chipotle-prod-rep-r68r6`, Safe-owned `app_id` |
| Staging domain | `test.chipotle.litprotocol.com` (prod domain = `vars.DOMAIN_PROD`) |
| Deploy trigger | push `next` → `deploy-staging.yml`; `v*` tag → `deploy-prod-1/2/3` |
| Cutover today | none — in-place restart of the one live CVM (downtime) |
| Target override | `phala_cvm_name` `workflow_dispatch` input (#434) lets a manual run target a specific instance |
| DNS / TLS | each CVM's `dstack-ingress` obtains an LE cert (DNS-01 via Route 53) **and auto-creates the routing DNS records** for `DOMAIN` |

Relevant files: `.github/workflows/deploy-staging.yml`, `deploy-prod-1-propose.yml`, `deploy-prod-2-execute-manual.yml`, `wait-for-api-restart.yml`, `manual_phala-add-device.yml`, `lit-api-server/blockchain/lit_node_express/tasks/propose-add-device.ts`, `docker-compose.phala.yml`, `justfile.deploy`.

---

## 3. How the cutover works (the load-bearing mechanism)

From the `dstack-ingress` source (`scripts/entrypoint.sh`, `dnsman.py`) + empirical testing: on **boot**, for its `DOMAIN`, the ingress writes three Route 53 records and obtains a cert:

| Record | Name | Value | Purpose |
|---|---|---|---|
| **ALIAS/CNAME** | `<domain>` | `_.dstack-base-prodN.phala.network` | points domain at that node's gateway |
| **TXT** | `_dstack-app-address.<domain>` | `<app_id>:443` | gateway reads this to route the domain to the app |
| **CAA** | `<domain>` | LE + (optionally) an `accounturi` restriction | issuance policy (`SET_CAA=false` to skip) |

Request flow: client → CNAME → node gateway → gateway reads the TXT, finds the app's authorized instance **on that node**, TLS-passthrough to the CVM's ingress → terminates TLS with its `<domain>` cert → proxies to `lit-api-server:8000`.

**The cutover is the auto-claim itself.** When the cold node's CVM boots, its ingress rewrites the ALIAS (→ its own node's gateway) + TXT. That *is* the flip — no CI-managed Route 53 needed. Cert issuance is via DNS-01 (`_acme-challenge` TXT), independent of routing, so the cold node gets a valid cert before/while it claims.

**Why one `app_id` works across nodes — the on-chain device allowlist.** Each Phala node has a distinct `device_id`. The app's on-chain **AppAuth/DstackApp** contract has an `addDevice(deviceId)` allowlist, and the **KMS only releases the app's keys to allowlisted devices** (`manual_phala-add-device.yml`, `propose-add-device.ts`). Allowlist *both* nodes' devices and a single-`app_id` instance runs as a full citizen on either node — derives the same keys **and** the gateway serves its custom domain. This is what makes ping-pong possible without separate apps.

> Correction to an earlier draft: a quick `phala cvms replicate` test (which does **not** run `addDevice`) produced a replica the gateway refused to serve the custom domain for, which wrongly looked like "single app_id is impossible / need two separate apps." The real cause was the missing on-chain device authorization. The manual prod cutover on 2026-06-02 (prod4→prod2, same `app_id`) proves the single-`app_id` model works when the device is allowlisted.

---

## 4. Architecture

One app per environment, `app_id` fixed. Two long-lived instances, one per node, **both devices allowlisted on-chain**. Exactly one is "live" (owns the domain) at a time.

```
              test.chipotle.litprotocol.com   ← owned by whichever node last claimed it
                                 │  (ALIAS + _dstack-app-address TXT, written by the live CVM's ingress)
              ┌──────────────────┴──────────────────┐
              ▼ (when prod2 live)                    ▼ (when prod4 live)
 ┌──────────────────────────────────┐   ┌──────────────────────────────────┐
 │ NODE prod2                        │   │ NODE prod4                        │
 │ same app_id, device allowlisted   │   │ same app_id, device allowlisted   │
 │ default URL: <appid>-8000         │   │ default URL: <appid>-8000         │
 │   .dstack-base-prod2…             │   │   .dstack-base-prod4…             │
 │ holds cert for the custom domain  │   │ holds cert for the custom domain  │
 └──────────────────────────────────┘   └──────────────────────────────────┘
        ▲ CI verifies the cold node here (default Phala URL) before cutover
```

- **Same `app_id` on both nodes** → one identity, shared derived keys; clients/contracts see a stable node. (This is why we don't use two separate apps.)
- **The live node owns the domain**; the gateway serves the domain from exactly **one** instance (it does **not** load-balance across instances — verified). Switching = the other node re-claiming on boot.
- **Verification uses the default Phala URL** (`<app_id>-8000.dstack-base-prodN…`), served by the gateway's own wildcard cert — reachable before the custom domain is involved. `wait-for-api-restart.yml` already resolves this via `phala cvms get`.

---

## 5. The deploy / cutover flow

```
deploy (push to next, or tag for prod)
  └─ build (unchanged: 3 images, sigstore-signed, digest-pinned)
  └─ select-target: read "last live node" → target the COLD node's CVM
        (manual override available via phala_cvm_name)
  └─ deploy new code to the COLD node's CVM
        (its ingress will auto-claim the domain on boot)
  └─ wait-for-api-restart  (phala_app_name = cold CVM, expected_sha)  ← default Phala URL
  └─ verify-attestation / openapi-spec-check / k6-smoke  ← against the cold default URL
  └─ confirm cutover: poll the PUBLIC domain until it serves the new SHA + /health  (the ingress claim has propagated)
  └─ shut down the OLD node's CVM   (only after the above is green)
  └─ record the new live node (so the next deploy targets the other one)
```

- **All gating happens on the cold node's default Phala URL before we depend on the cutover** — the new code is proven healthy before traffic shifts.
- **Don't stop the old node until the public domain is confirmed serving the new node.** The old node is the rollback target and the bridge for cached-DNS clients during rollover.
- prod keeps its Safe / on-chain compose-hash governance gate **before** the deploy; the cutover is the same auto-claim mechanism after verification.

### The health-gate subtlety (the one real risk)

The ingress claims the domain *on boot*, and `docker-compose.phala.yml` currently has `dstack-ingress depends_on lit-api-server: condition: service_started` (**not** `service_healthy`). So the domain can flip to the new node before `lit-api-server` is actually ready → a window where DNS-refreshed clients hit a not-ready box. Two mitigations, do both:

1. **Add a healthcheck to `lit-api-server`** and change the ingress to `depends_on: { lit-api-server: { condition: service_healthy } }` so the claim only happens once the backend is serving.
2. **Keep the old node running** until CI confirms the public domain serves the new SHA (cached-DNS clients stay on the healthy old node during rollover; low DNS TTL shrinks the window).

---

## 6. Tracking which node is live

The deploy must pick the **cold** node. Both instances share one `app_id`, so the `_dstack-app-address` TXT (=`app_id:443`) can't tell them apart — track by **node / CVM name**, not app_id:

- **Stored pointer (recommended):** a GitHub Actions variable (e.g. `NEXT_LIVE_CVM`) updated at the end of each successful deploy; deploy targets "the other one." Simple, explicit.
- **Cross-check from reality:** the live ALIAS (`_dstack-app-address.<domain>` resolves to the node gateway in the CNAME) tells you which **node** currently serves; confirm the stored pointer matches before trusting it.
- `phala_cvm_name` (#434) remains the manual override / break-glass.

---

## 7. CI changes

**`deploy-staging.yml`** (and the prod execute workflow):
- `select-target` step: read `NEXT_LIVE_CVM` (or live DNS) → set the cold CVM as the deploy target (instead of the hardcoded per-branch default).
- After `wait-for-api-restart` + verification on the cold default URL, add a **confirm-cutover** step (poll public domain for new SHA/health) and a **shutdown-old** step (`phala cvms stop`/delete the previously-live CVM), then **update `NEXT_LIVE_CVM`**.
- Keep the existing gates (`verify-attestation`, `openapi-spec-check`, `k6-smoke`, `k6-correctness`); point the pre-cutover ones at the cold default URL.

**`docker-compose.phala.yml`:** add the `lit-api-server` healthcheck + ingress `service_healthy` dependency (§5).

**Provisioning (one-time):** ensure both nodes' `device_id`s are allowlisted on the app's AppAuth contract (`manual_phala-add-device.yml` for EOA-owned `next`/`dev`; `propose-add-device.ts` → Safe for prod). Keep both CVMs long-lived so each cold deploy is an **in-place upgrade** (cert volume persists — see §8).

---

## 8. Robustness notes

- **Let's Encrypt rate limit — keep CVMs long-lived.** LE caps **5 certs/week per exact hostname**. A *fresh* CVM boot issues a new cert; an **in-place upgrade reuses the persisted `cert-data` volume** (no reissue). So: deploy by upgrading the two standing CVMs in place, *not* by reprovisioning fresh CVMs each time. Only node *migrations* (new device) should issue a cert — well under 5/week. (We hit this limit during testing by reprovisioning the same hostname repeatedly.)
- **Rollback** = bring the old node back / let it re-claim (it still has its cert and is an allowlisted instance), or just don't stop it until the new node is green. Sub-minute.
- **Split-brain:** during rollover both nodes are briefly up serving the same app/keys. Fine for stateless request serving; if any single-writer assumption exists (e.g. a leader-only background loop), confirm it tolerates a brief overlap, or shut the old node promptly after cutover.
- **CAA:** keep `SET_CAA=false` (or a CAA with no `accounturi` lock) so either node's ACME account can always (re)issue — verified the `chipotle`/`dev` zones currently have **no** `accounturi` restriction.
- **Cost:** two warm CVMs per environment ≈ 2× compute. Accepted tradeoff for zero downtime.

---

## 9. What we proved (Phase 0, dev zone, 2026-06-01→02)

All in the isolated `dev.litprotocol.com` zone with a least-privilege key; never touched `chipotle`/staging/prod.

- ✅ **Cert coexistence** — two independent ACME accounts both issued for one hostname (DNS-01); CAA is not a blocker (no `accounturi` lock).
- ✅ **In-CVM cert issuance via assumed role works** — `dstack-ingress` got a real LE cert using a dev-scoped IAM role (`CERTBOT_AWS_ROLE_ARN`).
- ✅ **Custom-domain routing works on `dstack-base-*` gateways for our own app** (a fresh `--kms base` primary served the custom domain). It does **not** work on `dstack-pha-*` gateways (the `--kms phala` default tier) — the real apps are `--kms base`, so this is fine.
- ✅ **The auto-claim cutover is real** — a booting CVM's ingress rewrites ALIAS+TXT to itself; DNS reaches authoritative INSYNC in seconds; rollback (re-point) is symmetric.
- ✅ **Gateway serves a domain from one instance, not load-balanced** — 40/40 requests hit the primary even with a healthy same-node co-tenant. (Good: ping-pong wants single-owner, switched by re-claim.)
- ❌→✅ **`phala cvms replicate` (no `addDevice`) replica couldn't serve the custom domain** for 8+ min. Root cause = missing on-chain device authorization, **not** a fundamental limit. The manual prod cutover (same `app_id`, allowlisted device) works — hence the single-`app_id` design here.
- ⚠️ **LE 5/week/hostname rate limit** is real and easy to hit when iterating — drove the "long-lived CVMs / in-place upgrade" rule in §8.

---

## 10. Open questions / confirm before automating

1. **Cutover health timing** — confirm adding the `lit-api-server` healthcheck + ingress `service_healthy` dependency actually delays the domain claim until the API is ready (test on a dev box).
2. **`phala cvms stop` vs `delete` for the old node** — stop (keep as warm rollback, cheaper than redeploy) vs delete. Prefer **stop** so the cert volume + identity persist for the next flip back.
3. **Both prod nodes' devices allowlisted?** Verify prod2 **and** prod4 `device_id`s are on the prod AppAuth allowlist (Safe-owned) before relying on prod4↔prod2.
4. **`NEXT_LIVE_CVM` source of truth** — Actions variable vs derived-from-DNS; pick one and make the deploy fail loudly if they disagree.
5. **Background/singleton work** — does anything in `lit-api-server` assume a single live instance (triggers, listeners)? Affects the split-brain note (§8).

---

## 11. Phased rollout

**Phase 0 — de-risk (done).** §9. Mechanism, cert behavior, auto-claim, and the device-allowlist requirement all validated in dev; manual prod cutover confirms the single-`app_id` flow end-to-end.

**Phase 1 — staging automation.** Add the healthcheck + `service_healthy` dependency to `docker-compose.phala.yml`. Rewrite `deploy-staging.yml` to select the cold node, deploy in place, verify on the default URL, confirm public cutover, stop the old node, record `NEXT_LIVE_CVM`. Dogfood on `next` with intentional alternating deploys.

**Phase 2 — production.** Apply the same flow to `deploy-prod-2-execute-manual.yml`, keeping the Safe/on-chain compose-hash gate before deploy and the device allowlist for both prod nodes. Cutover stays the auto-claim; rollback = don't-stop-old-until-green.

---

## 12. Task checklist

- [ ] Confirm prod2 **and** prod4 `device_id`s are allowlisted on each app's AppAuth contract (`manual_phala-add-device.yml` / `propose-add-device.ts`).
- [ ] `docker-compose.phala.yml`: add `lit-api-server` healthcheck + ingress `depends_on: service_healthy`.
- [ ] `deploy-staging.yml`: add `select-target` (cold node from `NEXT_LIVE_CVM` / live DNS), keep verification on the cold default Phala URL.
- [ ] Add **confirm-cutover** (poll public domain for new SHA/health) + **stop-old-node** steps; update `NEXT_LIVE_CVM` on success.
- [ ] Ensure deploys are **in-place upgrades** of two standing CVMs (preserve `cert-data`; avoid LE rate limit).
- [ ] Rollback runbook: re-start / re-claim the old node (don't stop it until the new node is green).
- [ ] Port the same flow into `deploy-prod-2-execute-manual.yml` after the Safe/on-chain gate.
- [ ] Decide `phala cvms stop` vs `delete` for the old node (prefer stop).

---

## 13. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Domain claimed before new API is ready | `lit-api-server` healthcheck + ingress `service_healthy`; keep old node up until public domain confirmed |
| LE 5-cert/week/hostname limit | Long-lived CVMs, in-place upgrades (reuse `cert-data`); only migrations issue certs |
| Stale "which node is live" pointer | Cross-check `NEXT_LIVE_CVM` against live DNS; fail loudly on mismatch |
| New node's device not allowlisted | Allowlist both devices up front; verify before cutover (KMS won't release keys otherwise) |
| Split-brain during rollover | Brief overlap is fine for stateless serving; confirm any singleton/background work tolerates it; stop old node promptly |
| Doubled infra cost | Accepted for zero downtime |
