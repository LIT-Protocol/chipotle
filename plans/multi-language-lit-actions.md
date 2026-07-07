# Multi-Language Lit Actions — Feature Gating & Capabilities (CPL-345)

**Status:** Draft / plan
**Linear:** [CPL-345](https://linear.app/litprotocol/issue/CPL-345/plan-for-supporting-multiple-languages-for-lit-actions-in-cvm)
**Created:** 2026-07-07
**Depends on:** gVisor runner (PRs #557/#558, arch doc CPL-344 [`architectureDocs/gvisor-server.md`])
and the any-language design plan [`plans/microvm-action-runner.md`].

---

## What already exists (do not re-plan)

The hard part is done. The **gVisor any-language runner** (`lit-actions/gvisor-server`,
binary `lit_actions_gvisor`) already runs Lit Actions written in any language inside the
Phala TDX CVM:

- It speaks the **exact same gRPC op-loop** as the JS runner, on its own socket
  (`/tmp/lit_actions_gvisor.sock`). No protocol, op-handler, billing, or key-custody changes.
- lit-api-server routes **`POST /lit_binary_action`** to it (a `Client` whose only difference
  is `socket_path`), authorizing on the **server-derived checksum → CID**, never client input.
- Code arrives as a content-addressed **bundle**: a `tar`/`tar.gz` with a `lit.json` manifest
  (`entrypoint` argv, `runtime` string [currently *informational*], `env`). The v1 base rootfs
  image ships **all supported runtimes** (python3, sh, curl, jq, …). A per-execution `runsc`
  sandbox with an overlay tmpfs runs it; a preinstalled guest **`lit` CLI** bridges ops.

> ⚠️ These live on branches `bangalore-v1` / `binary-action-wiring`, **not yet on `origin/main`**.
> This plan's Phase 1+ work lands *after* those merge. Sequencing matters — see Phases.

**So the runner is not the problem.** CPL-345 is the **product/deployment surface** on top of it:
_which_ languages and _which_ methods are turned on _in a given environment_, and _how a client
discovers them_.

---

## Goal (from the ticket)

1. **Feature-gate** available languages **and methods** in the deploy YAML, so `next` (internal)
   and `production` can expose different sets.
2. Add a **lit-api-server endpoint** that describes the language features the server supports.
3. Each language feature descriptor carries:
   - **Language name**
   - **Runtime version(s)** — multiple may coexist (e.g. Python 3.12 *and* 3.13)
   - **Supports Direct Coding**
   - **Supports Bundle**

### The two methods, defined

| Method | Meaning | How it maps to the runner today |
|---|---|---|
| **Bundle** | Client submits a packaged project (tar + `lit.json`, arbitrary files/libs). | **Already implemented** — this *is* `/lit_binary_action`. |
| **Direct Coding** | Client picks a language+runtime and submits raw source (like JS today). | **New, thin.** For interpreted langs, the server **synthesizes a one-file bundle** (`main.py` + generated `lit.json` with `entrypoint: ["python3","main.py"]`) and forwards it down the *same* bundle path. No new runner logic. |

JavaScript is the degenerate case: **direct-coding only, no bundle**, served by the existing JS
runner. Including JS in the descriptor gives clients one unified capability list.

---

## Core design decision: two layers of truth, reconciled at boot

The ticket suggests build **tags that enable different packages in the Docker deploy**. That is a
*TCB-minimization* lever (the ticket's own table flags that interpreters enlarge the TCB, and the
rootfs is **measured/attested**). But we also need a fast *policy* lever to enable/disable a
language per environment without rebuilding and re-measuring images. Use **both, layered**, and
**reconcile them so they cannot drift**:

1. **Physical layer (build-time, measured).** Which runtimes are actually installed in the
   sandbox base rootfs image. Controlled by image build args / which image digest is deployed.
   This is the *maximum* possible set and part of the attestation.
2. **Policy layer (config, per-environment).** A declarative **language registry** file
   (`languages.<env>.toml`) selected by the deploy YAML — same pattern as `ARG NODE_CONFIG`
   selecting `NodeConfig.next.toml`. Lists, per language: enabled runtimes (name+version),
   `supports_direct_coding`, `supports_bundle`, and the internal entrypoint template used to
   synthesize direct-coding bundles.
3. **Reconciliation (startup handshake).** On boot, the gVisor runner **introspects its own
   rootfs** — either a manifest baked into the image at build time, or by exec'ing
   `python3 --version` etc. — and answers a new **`GetCapabilities` gRPC**. lit-api-server
   intersects *installed ∩ policy-enabled* and advertises **only that intersection**.

> This kills the classic failure mode where config claims Python but the image lacks it (or
> reports the wrong version). **Runtime versions come from the actual image, not hand-edited
> config** — so the descriptor is always truthful.

The capabilities endpoint therefore serves the **effective** set, computed once at startup
(cached), re-derivable on the runner's restart.

---

## Endpoint

Follow the existing `Configuration` convention (`lit-api-server/src/core/v1/endpoints/configuration.rs`,
`OpenApiResponse`, mounted under `/core/v1/`):

```
GET /core/v1/configuration/get_supported_languages   ->   SupportedLanguagesResponse
```

```jsonc
{
  "languages": [
    {
      "name": "javascript",
      "runtimes": [{ "name": "deno", "version": "2.8.0" }],
      "supports_direct_coding": true,
      "supports_bundle": false
    },
    {
      "name": "python",
      "runtimes": [
        { "name": "cpython", "version": "3.13.1" },
        { "name": "cpython", "version": "3.12.8" }
      ],
      "supports_direct_coding": true,
      "supports_bundle": true
    }
  ]
}
```

- New model `SupportedLanguagesResponse` + `LanguageFeature` + `RuntimeVersion` in
  `core/v1/models/response.rs` (derive `JsonSchema` for the OpenAPI/swagger surface, like
  `LitActionClientConfigResponse`).
- Handler is thin; the effective list is assembled in `core_features.rs` from the reconciled
  registry (cached at startup).
- **CI gate:** a new Rocket route means the **k6 client check** fires — regenerate
  `k6/litApiServer.ts` after adding the route (see project memory on CI gates).

---

## Request path for Direct Coding

Bundle already has `/lit_binary_action`. Direct coding needs a language selector. **Recommended:**
extend the entry so a language tag routes the request:

- `POST /lit_action` with an optional `language` (default `javascript`) + optional `runtime`
  (e.g. `"3.13"`). `javascript` → existing JS runner unchanged. Any other → server validates the
  language/runtime/method against the effective capability set, **synthesizes a single-file
  bundle**, and forwards to the gVisor socket via the existing binary path.
- Bundle requests keep going to `/lit_binary_action`; make its `lit.json.runtime` field
  **meaningful** (validated against available runtimes) instead of informational, so bundles can
  pin a version too.

**Admission control (cheap, important):** reject any request whose (language, runtime, method) is
not in the effective set, with a precise 4xx (`"language 'python' not enabled on this node"` /
`"direct coding not supported for 'python'"` / `"runtime '3.11' unavailable; have 3.12, 3.13"`).
Never silently fall back.

_Alternative considered:_ a dedicated `POST /lit_action_code?language=…` sibling. Rejected —
duplicates the JS entry's guards/billing and splits the client story. Extending the existing entry
keeps one code path and one set of guards.

---

## Feature gating in the deploy YAML (concretely)

Two knobs, mirroring the existing `NodeConfig`/`compose_hash` machinery:

1. **Policy (fast, per-env):** `Dockerfile.lit-api-server` gains
   `ARG LANGUAGES_CONFIG=languages.next.toml` (parallel to the existing `ARG NODE_CONFIG`);
   the workflow sets it per environment. Because the config is copied into the measured image,
   the enabled set is attested.
2. **Physical (TCB, governed):** `DOCKER_IMAGE_LIT_ACTIONS_GVISOR` digest selects the rootfs
   variant. `next` can run a fat image (all experimental runtimes); `production` can run a slim,
   audited image. Adding/altering the `lit-actions-gvisor` compose service or its image changes
   **`compose_hash`**, which is **governed in prod** — call this out in the deploy runbook.

Net effect the ticket asks for: `next` exposes, say, `{js, python(bundle+direct)}` while prod
exposes `{js}` until Python clears review — flipped by config (and, when TCB matters, by image).

---

## Implementation phases

| Phase | Deliverable | Notes / gates |
|---|---|---|
| **0 (prereq)** | Merge gVisor runner + `/lit_binary_action` (PRs #557/#558). | Not this ticket, but Phase 1+ stacks on it. |
| **1** | Language **registry config** + `GetCapabilities` gRPC on the gVisor runner + reconciliation at boot + **`get_supported_languages`** endpoint. Advertises `js` + `python(bundle)`. | Read-only surface first. Regen `k6/litApiServer.ts`. |
| **2** | **Direct coding** for Python: `language`/`runtime` on the request, bundle synthesis, admission control + errors. Make `lit.json.runtime` validated. | Reuses the bundle runner entirely. |
| **3** | **Deploy-YAML gating**: `ARG LANGUAGES_CONFIG` + per-env `languages.*.toml`; slim vs fat rootfs image selection; document `compose_hash` governance. | Prod stays JS-only until sign-off. |
| **4** | **Multiple runtime versions** (3.12 + 3.13): version→interpreter mapping in the image manifest + entrypoint templates; endpoint lists all. | Only if product needs pinning. |

Phases 1–2 deliver the ticket's endpoint + Python on `next`. Phase 3 delivers the environment
split. Phase 4 is optional depth.

---

## Testing

- Endpoint contract test (shape, JS always present).
- **Drift test:** config enables a language the runner reports absent → it is **not** advertised.
- Version-truthfulness test: advertised version == runner-reported version.
- Admission rejection tests: disabled language, unsupported method, unavailable runtime version.
- Direct-coding synthesis test: raw Python → synthesized `lit.json`/`main.py` → op-loop round-trip
  (reuse the existing Python hello-world bundle test harness, `process` runtime for CI).
- Per project memory: run gVisor/lit-actions integration tests with `NO_COLOR=1`.

---

## Open questions

1. **Registry location** — dedicated `languages.<env>.toml`, or a `[languages]` block inside
   `NodeConfig.*.toml`? (Leaning dedicated file; cleaner per-env diff + governance.)
2. **Version introspection** — bake a manifest into the rootfs image at build time (fast,
   deterministic, part of measurement) vs. exec `--version` at boot (self-updating, but adds boot
   probes). Leaning baked manifest.
3. **Is direct-coding source itself content-addressed/authorized?** The synthesized bundle should
   flow through the same server-derived-checksum authorization as `/lit_binary_action`, so the
   `can_execute_action` check still holds. Confirm the CID is derived post-synthesis.
4. **Should the JS runner also answer `GetCapabilities`** for uniformity, or does api-server just
   hard-code the JS entry (version from the Deno pin)? Leaning hard-code JS (it's not going to
   drift within a build).
5. **Bundle-only vs direct-only per language** — is any language bundle-only (e.g. a compiled
   lang where "direct coding" makes no sense)? The descriptor already models this; confirm the
   product wants it exposed.
