# Agent Context: Load & Correctness Tests (k6 / TypeScript)

## Purpose
k6 performance and correctness tests for `lit-api-server`. The HTTP client
(`litApiServer.ts`) is **auto-generated** from the OpenAPI spec via
`@grafana/openapi-to-k6` — do not hand-edit it. Layout: `correctness/` (functional
specs — integration, api-key-security, billing, Lit Action signing), `loadtest/`
(soak, spike, breakpoint), `LitActionCode/` (JS payloads executed under test),
`data/` (pre-seeded account pools), `baselines/` (soak baselines). Shared helpers in
`helpers.ts`, `setup.ts`, `defaults.ts`, `stripe.ts`.

## Stack & Tooling
- Runner: [k6](https://grafana.com/docs/k6/) (Grafana). Tests are TypeScript, run with `k6 run <spec>`.
- Client generation: `@grafana/openapi-to-k6` (Node.js) regenerates `litApiServer.ts` from the OpenAPI spec.
- Task runner: `just -f k6/justfile <recipe>` (smoke/soak/spike/breakpoint, `-local` variants, account seeding).

## Coding Rules
- Never hand-edit `litApiServer.ts` — it is generated. Regenerate it whenever a Rocket route / the OpenAPI spec changes (this is enforced by the `k6-client-check` CI gate).
- The OpenAPI spec paths omit the `/core/v1` prefix; set `BASE_URL` with `/core/v1` when targeting the Phala deployment.
- Reuse the pre-seeded account pool (`data/accounts.json`, seeded via `just -f k6/justfile seed-accounts`) instead of creating accounts in `setup()` where possible, to reduce system noise.
- Keep secrets out of committed specs and `data/` fixtures; pass them via environment variables (`BASE_URL`, `ACCOUNTS_FILE`, etc.).

## Definition of Done
1. After any change to a Rocket route or the OpenAPI surface, regenerate `litApiServer.ts` and commit it (keeps `k6-client-check` green).
2. Smoke test passes: `just -f k6/justfile smoke-local` (or `k6 run k6/smoke.spec.ts`).
3. Relevant correctness specs pass against a running stack.
4. For load specs, compare results against the checked-in `baselines/` and update them deliberately (`update-soak-baseline.sh`) when a shift is intended.
