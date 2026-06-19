# Gate releases on k6 load tests (perf regression gate)

**Status:** Phase 1 shipped (PR #506). Gate vehicle switched spike → low-VU soak after a live test (see §3.1). Phase 2 (delta gate vs committed baseline) shipped (see §4). Phase 3 not started.
**Scope:** Turn the existing k6 load tests (`k6/loadtest/{spike,soak,breakpoint}.spec.ts`) from manual-only tools into an automated perf-regression gate on the staging deploy, then into real run-over-run regression detection. Dedicated idle servers are earmarked as the stable load-generation / baseline hardware.

---

## 1. Goal

Stop perf regressions from shipping. Today the load tests only run by hand (`just -f k6/justfile spike-local` etc.) and store nothing — no baseline, no history, no gate. We want a build that got slower (or falls over under load) to fail CI before it becomes the live staging build, and eventually to fail on a *delta* vs the last good release, not just an absolute ceiling.

---

## 2. Current state

| Thing | Today |
|---|---|
| Load specs | `k6/loadtest/{spike,soak,breakpoint}.spec.ts` — real Lit Actions (ECDSA sign, encrypt/decrypt), spend test-mode Stripe credits via `ensureAccountCredits` |
| Thresholds | Absolute SLOs only (spike `checks>=0.8`, `p99<30s`; soak `p99<15s`; breakpoint `p95<2s` abortOnFail). No deltas. |
| Accounts | Pre-funded pools `k6/data/accounts.{next,dev}.json` (~50 each), reused across runs |
| Results storage | None for `-local`; `--out cloud` streams to Grafana Cloud k6 only if run that way |
| CI wiring | Phase 1 (below) wired `spike` into `deploy-staging.yml` |

---

## 3. Phase 1 — absolute-threshold gate ✅ DONE (PR #506)

- Added `.github/workflows/k6-loadtest.yml`: reusable (`workflow_call`) + `workflow_dispatch`, mirrors `k6-correctness.yml` conventions (self-hosted, `grafana/setup-k6-action@v1`, `accounts_json`→tempfile, branch-based `K6_ACCOUNTS_FILE`). Runs `spike|soak|breakpoint|smoke`; one generic `vus`/`duration`/`steps` input maps to each spec's per-test env vars. Captures the k6 textSummary to a build artifact via `tee` + `set -o pipefail` (keeps k6's exit code = the gate).
- Wired into `deploy-staging.yml` as job `k6-loadtest` (test: `spike`) after `k6-correctness`, against the **cold box's direct URL**. Added to `confirm-cutover` `needs` (blocks cutover on fail) and `rollback-on-failure` `needs`+condition.
- Gate is pass/fail on each spec's ABSOLUTE thresholds only.
- Codex-review fixes folded in: `env` (staging|prod) dispatch input so manual prod runs don't enable Stripe top-ups; corrected the gate comment (cold box auto-claims DNS at boot → this is detect-and-roll-back, not prevent-go-live); added `k6-loadtest.yml`, `k6/loadtest/`, `k6/setup.ts` to `detect-changes` deploy paths.

**Known limitation (by design for P1):** the gate is detect-and-roll-back. The cold box auto-claims the public domain at container boot, so a bad build can briefly serve public traffic before the gate fails and `rollback-on-failure` flips DNS back.

### 3.1 Gate vehicle: spike → low-VU soak (chosen after a live test)

The first dispatch ran `test: spike` (25 VUs of real crypto) against next and got a flood of HTTP 429s. Root cause: that load saturates the small `tdx.small` box and trips lit-api-server's CPU load-shedding guard (`cpu_overload.rs`, sheds at load>2×cpu or CPU PSI>50%). Spike measures the *failure cliff*, not latency — and worse, the shed 429s return instantly, which would *lower* measured latency and **mask** "got slower" regressions. So spike is the wrong vehicle for "we made this endpoint a lot slower."

Switched the gate to a **low-VU soak** (`test: soak`, `vus: 2`, `duration: 3m`, `scenario: soak`). A 2-VU soak with soak's 2–4s think time runs at ~2 req/s — well under the shed threshold — so per-endpoint p95 reflects the code path, not contention. `SCENARIO=soak` (new `k6-loadtest.yml` input) drops the spec's `ramp_*` scenarios (one of which spins a hot no-op VU on the runner).

**Verified clean on next (run 27782975924):** 0 shedding 429s, `http_req_failed 0.00%`, `checks 100%`. First baseline numbers (low-load p95):

| Endpoint | avg | p95 | max |
|---|---|---|---|
| `soak_encrypt_decrypt` | 99 ms | **106 ms** | 207 ms |
| `soak_ecdsa_sign` | 249 ms | **274 ms** | 322 ms |

`soak_*` and `ramp_*` agreed within a few ms → reproducible signal.

**Interim absolute ceilings (in `soak.spec.ts`, env-overridable):** `SOAK_P95_ENCRYPT_MS=350`, `SOAK_P95_ECDSA_MS=700` (~3× baseline). Catches gross regressions without false-failing on jitter; Phase 2 replaces with deltas.

**Account provenance (fixed):** a validation run failed with 401 "key not recognized" — the committed `accounts.next.json` keys didn't resolve on the instance serving at that moment (they resolved fine ~15 min later). A gate that runs against a freshly-deployed / cold instance can't depend on a pre-seeded pool existing there. Fix: `SOAK_CREATE_ACCOUNTS=true` (new `create_accounts` workflow input, set by the gate) makes `soak.spec.ts` create ephemeral accounts via `createAccountAndUsageKey` against the actual target box in `setup()`, like the correctness specs do. Manual runs default to the pre-seeded pool. **Open question (possible prod concern, not yet investigated):** why valid accounts stopped resolving — is account/API-key state per-instance and not replicated across the ping-pong pair (→ real users 401 after cutover), or just cutover-timing? See [[pingpong-deploy-cvm-name-param]].

**Open knob:** the soak scenario's hardcoded 2m ramp-up/down makes the gate ~7 min. If that's too slow per deploy, trim the ramps (→ ~4 min) or run on a dedicated box (Phase 3).

---

## 4. Phase 2 — delta gate vs committed baseline ✅ SHIPPED

Gate on a **delta vs a committed baseline**, not absolute thresholds. Baseline is anchored in a committed file (not the last artifact) so cumulative drift is caught, and refreshed manually.

- **Structured JSON output.** `soak.spec.ts` `handleSummary` now writes `soak-summary.json` (per-scenario p50/p95/p99/avg/max + check & failure rates). `summaryTrendStats` extended to include p(99). This file is both the per-run artifact and the exact shape the baseline takes.
- **Committed baseline:** `k6/baselines/soak.next.json` (a saved summary). The deploy gate passes `baseline_file: ../baselines/soak.next.json` (path is relative to `k6/loadtest/` — where the spec's `open()` resolves).
- **Delta gate (in-spec, reuses k6 thresholds).** When `SOAK_BASELINE_FILE` is set, each endpoint's p95 threshold is derived: `p95 < max(baseline*(1+SOAK_P95_TOLERANCE), baseline + SOAK_P95_FLOOR_MS)` (defaults 0.30 / 50ms — the floor stops ~60-130-sample jitter from false-failing). k6's exit code = the gate, so it flows through the existing cutover-block + rollback plumbing with no new job. No baseline / unreadable → falls back to the absolute `SOAK_P95_*_MS` ceilings.
- **Manual baseline refresh:** `k6/update-soak-baseline.sh` runs a measurement soak against staging (create_accounts, no baseline → doesn't gate itself) and writes `k6/baselines/soak.next.json`. Run it + commit via PR when perf legitimately changes (e.g. got substantially faster). Per the decision: baseline updates are a deliberate manual op, not automated.

**Folded-in Codex findings:** #4 (loose thresholds) — resolved: the gate is now a baseline-relative delta. #2 (time-to-rollback: rollback waits the full ~7-min soak even if an earlier gate failed) — **still open**, deferred to a fast-fail-decoupling change.

**Not done (optional):** prod baseline gating (the manual prod soak still just measures with high ceilings); k6→Prometheus/Grafana history (the committed file + per-run artifacts suffice for the gate).

---

## 5. Phase 3 — stabilize the signal (TODO)

- Pin load tests to the dedicated idle servers only; fixed VU/duration; warm-up before measuring so baselines aren't noisy.
- Add a nightly cron (against staging) in addition to the release gate — spots drift between releases and builds baseline history for Phase 2's deltas.

---

## 6. Loose ends / footguns to keep in mind

- Tests spend test-mode Stripe credits and do real signing — never gate against prod.
- `just seed-accounts` greps `^{"generated_at"` but `accounts.seed.spec.ts` pretty-prints JSON, so the grep matches nothing; the committed account files were generated some other way. Fix before relying on re-seeding (Codex #6-adjacent).
- Committed account-pool depletion/staleness would surface as a deploy rollback rather than a controlled failure (Codex #5).
