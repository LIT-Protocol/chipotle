# Load Tests

Long-duration and stress tests for lit-api-server.

## Spike Test

**Spike test**: sudden load increase to verify the system handles traffic spikes and recovers.

```bash
# Default: 20 VUs, 2 min sustain
k6 run k6/loadtest/spike.spec.ts

# Heavier spike
SPIK_VUS=50 SPIK_DURATION=3m k6 run k6/loadtest/spike.spec.ts

# Custom base URL
BASE_URL=http://localhost:8000/core/v1 k6 run k6/loadtest/spike.spec.ts
```

**Environment variables:**

| Variable       | Default | Description              |
|----------------|---------|--------------------------|
| `BASE_URL`     | test.chipotle.litprotocol.com | API base URL          |
| `SPIK_VUS`     | `20`    | Peak virtual users       |
| `SPIK_DURATION`| `2m`    | Sustain duration at peak  |

Stages: 10s ramp-up → sustain → 10s ramp-down. High request rate (minimal sleep between requests).

---

## Soak Test

**Soak test** (endurance/stability): long-duration, low-intensity load to detect memory leaks, resource exhaustion, and gradual performance degradation.

```bash
# Default: 1h soak, 3 VUs
k6 run k6/loadtest/soak.spec.ts

# Shorter run (30 min)
SOAK_DURATION=30m k6 run k6/loadtest/soak.spec.ts

# Custom base URL
BASE_URL=http://localhost:8000/core/v1 k6 run k6/loadtest/soak.spec.ts

# More VUs (still low intensity)
SOAK_VUS=4 k6 run k6/loadtest/soak.spec.ts
```

**Environment variables:**

| Variable        | Default | Description                    |
|----------------|---------|--------------------------------|
| `BASE_URL`     | test.chipotle.litprotocol.com | API base URL (include `/core/v1`) |
| `SOAK_DURATION`| `30m`    | Steady-state duration          |
| `SOAK_VUS`     | `3`     | Virtual users                  |

Total test time ≈ 4 minutes (ramp-up/down) + `SOAK_DURATION`.

---

## Breakpoint Test

**Breakpoint test**: step-wise increasing load up to a maximum number of VUs (default 30) to find the point where latency, error rate, or resource usage becomes unacceptable.

```bash
# Default: 1 → 5 → 10 → 20 → 30 VUs, 2m per step
k6 run k6/loadtest/breakpoint.spec.ts

# Custom max VUs (steps clamped to this value)
BPT_MAX_VUS=40 k6 run k6/loadtest/breakpoint.spec.ts

# Longer steps (3 minutes at each VU level)
BPT_STEP_DURATION=3m k6 run k6/loadtest/breakpoint.spec.ts

# Explicit steps (last value treated as breakpoint/max)
BPT_STEPS=1,3,6,12,24 k6 run k6/loadtest/breakpoint.spec.ts

# Custom base URL
BASE_URL=http://localhost:8000/core/v1 k6 run k6/loadtest/breakpoint.spec.ts
```

**Environment variables:**

| Variable           | Default | Description                                   |
|--------------------|---------|-----------------------------------------------|
| `BASE_URL`         | test.chipotle.litprotocol.com | API base URL (include `/core/v1`) |
| `BPT_MAX_VUS`      | `30`    | Maximum/peak VUs for the test                 |
| `BPT_STEP_DURATION`| `2m`    | Duration for each load step                   |
| `BPT_STEPS`        | *none*  | Optional comma-separated list of VU levels    |

---

## Performance regression gate (soak) + baseline

Staging deploys are gated on a **low-VU soak** that fails the deploy if an
endpoint got meaningfully slower. The gate runs in
`.github/workflows/deploy-staging.yml` (via the reusable `k6-loadtest.yml`)
against the freshly-deployed instance, after smoke + correctness and before the
ping-pong cutover. A failure blocks cutover and triggers `rollback-on-failure`.

Why soak and not spike: 25-VU spikes saturate the small staging box and trip the
API's CPU load-shedding (429s), which measures the failure cliff, not latency —
and the fast 429s would *mask* a regression. A 2-VU soak stays under that
threshold so per-endpoint p95 reflects the code path.

### How the gate decides pass/fail

The gate compares the run's per-endpoint **p95** against a **committed baseline**
(`k6/baselines/soak.next.json` — a saved run summary). For each endpoint it
fails if:

```
p95 > max(baseline_p95 * (1 + SOAK_P95_TOLERANCE), baseline_p95 + SOAK_P95_FLOOR_MS)
```

Defaults: `SOAK_P95_TOLERANCE=0.30` (30%) and `SOAK_P95_FLOOR_MS=50`. The floor
keeps the ~60–130-sample p95 noise from false-failing on tiny absolute moves.
With the seeded baseline (encrypt p95 ~112ms, ecdsa ~274ms) the gate trips at
roughly encrypt > 162ms / ecdsa > 356ms.

Anchoring on a *committed* baseline (not the previous run) is deliberate: it
catches **cumulative** drift. Comparing only to the last run would let a steady
few-percent-per-release creep pass forever and compound.

**Fail-closed:** when a baseline IS requested (`SOAK_BASELINE_FILE` set, as the
deploy gate does), the run errors red if the baseline can't be opened, isn't
valid JSON, has no positive p95 for a running endpoint, or the tolerance/floor
are invalid. It never silently downgrades to the absolute ceilings — a broken
baseline must not let a regression pass green. The absolute ceilings
(`SOAK_P95_ENCRYPT_MS` 350 / `SOAK_P95_ECDSA_MS` 700) apply only when no baseline
is requested (local / measurement runs).

Each run also uploads `soak-summary.json` (per-scenario p50/p95/p99/avg/min/max +
check & failure rates) as a build artifact — the same shape the baseline takes.

### Refreshing the baseline — `update-soak-baseline.sh`

Baseline updates are a **manual** operation. When perf legitimately changes (e.g.
a release made things substantially faster, so the old ceiling is needlessly
loose), regenerate and commit the baseline:

```bash
# Requires k6 installed locally. Runs a measurement soak against staging
# (creates its own test-mode-funded accounts) and writes the new baseline.
k6/update-soak-baseline.sh                  # network=next, 5m steady (~9m total)
DURATION=10m k6/update-soak-baseline.sh     # longer run → steadier p95
NETWORK=next BASE_URL=https://… k6/update-soak-baseline.sh

# Then review the numbers and commit via PR:
git add k6/baselines/soak.next.json
git commit -m "chore(k6): refresh soak baseline"
```

The script is a pure measurement run — it deliberately does **not** load a
baseline, so it never gates itself; it just measures and writes
`k6/baselines/soak.<network>.json`. Review the new p95s before committing (a PR
makes baseline changes reviewable, which is the point).

### Relevant environment variables

| Variable               | Default | Description                                                        |
|------------------------|---------|--------------------------------------------------------------------|
| `SOAK_VUS`             | `3`     | Total soak VUs (the gate uses `2`)                                 |
| `SOAK_DURATION`        | `30m`   | Steady-state duration (the gate uses `3m`)                         |
| `SCENARIO`             | *all*   | `soak` (the gate) runs only the steady scenarios; drops `ramp_*`   |
| `SOAK_CREATE_ACCOUNTS` | `false` | `true` → create ephemeral accounts in setup vs the pre-seeded pool |
| `SOAK_BASELINE_FILE`   | *none*  | Path (relative to `k6/loadtest/`) to a baseline summary to gate on |
| `SOAK_P95_TOLERANCE`   | `0.30`  | Allowed p95 growth vs baseline, as a **fraction** (0.30 = 30%, not 30) |
| `SOAK_P95_FLOOR_MS`    | `50`    | Absolute p95 cushion added on top of the tolerance                 |
| `SOAK_P95_ENCRYPT_MS`  | `350`   | Absolute encrypt/decrypt p95 ceiling (fallback when no baseline)   |
| `SOAK_P95_ECDSA_MS`    | `700`   | Absolute ecdsa-sign p95 ceiling (fallback when no baseline)        |

> Prod baseline: `manual_k6-prod-test.yml` can run the same soak against prod
> (`run_soak: true`) as a manual before/after-release latency check. It uses the
> pre-funded `K6_ACCOUNTS_PROD_JSON` pool and high p95 overrides (measurement
> only, not gated). See the project plan in `plans/k6-perf-regression-gate.md`.
