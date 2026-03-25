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
| `BASE_URL`     | api.dev.litprotocol.com | API base URL          |
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
| `BASE_URL`     | api.dev.litprotocol.com | API base URL (include `/core/v1`) |
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
| `BASE_URL`         | api.dev.litprotocol.com | API base URL (include `/core/v1`) |
| `BPT_MAX_VUS`      | `30`    | Maximum/peak VUs for the test                 |
| `BPT_STEP_DURATION`| `2m`    | Duration for each load step                   |
| `BPT_STEPS`        | *none*  | Optional comma-separated list of VU levels    |
