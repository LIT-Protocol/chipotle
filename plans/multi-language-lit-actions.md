# Multi-language Lit Actions (CPL-345)

## Motivation

Lit Actions today are JavaScript only: the client POSTs JS source to
`/lit_action`, and `lit-actions/server` runs it in a Deno/V8 isolate. With the
gVisor runner landing (PRs **#557**, **#558**, **#559**) we can now execute a
Lit Action written in **any language** inside a per-execution `runsc` sandbox,
speaking the *identical* gRPC op-loop as the JS runner. The keys never leave the
TEE; guest code reaches them only through the proxied `lit` CLI.

The remaining work — this plan — is the product surface on top of that runner:

1. **Two ways to run non-JS code** — a *raw script* path (pick a language +
   runtime, send code, like JS today) and a *bundle* path (ship a project,
   already delivered by #558).
2. **Feature-gating** the available `(language, runtime, method)` tuples in the
   deploy YAML, so `next` (internal) can expose more than production.
3. A **discovery endpoint** so clients can learn what a given node supports.
4. **Startup-script conventions** for passing PKP-derived secrets into Python
   and compiled (Rust) apps via the guest `lit` CLI, with worked examples.

We start with **Python** (interpreted, raw-script + bundle) and **Rust**
(compiled, bundle-only) as the reference languages.

## Prerequisites (delivered by other PRs — not in this branch)

> ⚠️ **The paths below do not exist on this branch.** They are delivered by the
> **open, not-yet-merged** PRs #557/#558/#559 and are listed here as
> prerequisites this plan builds on — see Phase 0. The plan is only valid once
> those land; nothing in this doc-only PR adds or modifies that code.

Recap of the machinery this plan relies on, so the scope below is only the
*delta*:

| Piece | Where (once #557/#558/#559 land) | Delivered by |
|---|---|---|
| gVisor any-language runner | `lit-actions/gvisor-server` (Action gRPC service + `runsc` supervisor) | #557 (open) |
| Guest `lit` CLI (ops from any language) | `lit-actions/gvisor-server/src/bin/lit.rs`, baked into sandbox rootfs at `/usr/local/bin/lit` | #557 (open) |
| Bundle format (`lit.json` manifest + tar) | `gvisor-server/README.md` | #557 (open) |
| `POST /lit_binary_action` (bundle path) | `lit-api-server` `core_features` + `endpoints/actions.rs`; request `LitBinaryActionRequest { bundle, checksum, js_params }` | #558 (open) |
| Socket wiring + compose service `lit-actions-gvisor` | `execution.rs`, `main.rs` (`LIT_ACTIONS_GVISOR_SOCKET`), `docker-compose.phala.yml` | #558 (open) |
| Sandbox rootfs image | `gvisor-server/image/Dockerfile.rootfs` (as of #557 ships `python3`, `jq`, `curl`, the `lit` CLI — **this plan strips the runtimes**, see Runtime provisioning) | #557 (open) |
| Architecture doc | `architectureDocs/gvisor-server.md` | #559 (open) |

The **wire contract is unchanged**: the bundle rides in `ExecutionRequest.code`
(base64 tar/tar.gz, or `cid:<id>`), `ipfs_id` is the server-derived checksum
used for permissions/key derivation, and ops (`GetPrivateKey`, `AesEncrypt`, …)
are proxied 1:1. Nothing in this plan changes the proto.

## Scope

**In scope**

- A raw-script path for non-JS languages (`POST /lit_raw_action`) that wraps
  submitted code into a bundle server-side and routes it to the gVisor runner.
- Making the manifest `runtime` field *authoritative* (select among multiple
  installed versions), not just informational.
- A deployment feature-gate: a declared `(language, runtime, method)` allowlist,
  validated at startup against what the sandbox image physically ships.
- A discovery endpoint `GET /get_supported_languages`.
- Startup-script conventions + worked Python and Rust examples that pass
  PKP-derived secrets into the app via the `lit` CLI.
- Docs under `docs/lit-actions/`.

**Non-goals (this plan)**

- No new runtimes beyond Python (interpreted) + a compiled-binary story (Rust
  as the example). Java/Go/C#/etc. are follow-ups that reuse the same gate.
- No changes to the op-loop proto, the JS runner, or key derivation.
- No warm pool / usage-based (per-`memory.peak`) billing — inherited follow-ups
  from #557; v1 stays flat per-second via the unchanged `UpdateResourceUsage`.
- No client-SDK ergonomics beyond the discovery endpoint (a typed SDK wrapper is
  a follow-up).

## The two execution methods

The ticket calls out two paths for interpreted languages. Generalized across all
languages:

| Method | What the client sends | How it runs | Example languages |
|---|---|---|---|
| **Raw script** | language + runtime + **code** (+ params) | server synthesizes a one-file bundle and runs it in gVisor (JS is the exception — it stays on the dedicated Deno runner) | JavaScript (Deno), Python |
| **Bundle** | a **tar/tar.gz** project with a `lit.json` manifest (+ params) | runs verbatim in gVisor | Python, Rust, Go, any compiled binary, shell |

Rule of thumb:

- **Interpreted languages** (Python) support *both* raw-script and bundle.
- **Compiled languages** (Rust, Go, C++) are **bundle-only** — you ship a
  prebuilt binary plus a startup script; there is no "send me source" path
  because compilation doesn't happen in the enclave.
- **JavaScript** is the special raw-script case that keeps its own Deno runner
  (`/lit_action`) for speed; it does not go through gVisor.

Everything except the JS raw-script path funnels through the single gVisor
runner and the single bundle contract, so adding a language is mostly *an install
recipe + gating* (see Runtime provisioning), not new execution machinery.

### Raw-script → bundle desugaring

`/lit_raw_action` is pure sugar over the bundle path. For a Python raw script the
server builds an in-memory tar, emitting the **provisioning line** for the chosen
runtime (see "Runtime provisioning") so the client never writes it:

```
main.py        <- the submitted code, verbatim
run.sh         <- `apt-get install -y python3.13=…` (cache hit) then `python3.13 main.py`
lit.json       <- { "entrypoint": "run.sh", "runtime": "python3.13" }
```

then hands it to the same code path as `/lit_binary_action` (derive checksum from
the tar bytes → authorize → run). No separate runner or auth path. The install
recipe + entrypoint per language/runtime come from the language registry (below),
so the client only picks `language` + `runtime`.

## Language feature model

One registry drives both the discovery endpoint and request admission. It is the
source of truth for "what does this node run".

```rust
// lit-api-server/src/actions/languages.rs (new)

/// One supported language, as advertised by GET /get_supported_languages.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LanguageFeature {
    /// Stable id used in requests, e.g. "python", "rust", "javascript".
    pub name: String,
    /// Human label, e.g. "Python".
    pub display_name: String,
    /// Underlying runner: "deno" (JS) or "gvisor" (everything else).
    pub execution_model: ExecutionModel,
    /// Provisionable runtime versions — each maps to an install recipe and a
    /// pre-warmed cache profile (NOT baked into the image). Multiple may
    /// coexist (e.g. 3.12 and 3.13). Empty for compiled/static languages.
    pub runtimes: Vec<LanguageRuntime>,
    /// Which methods this language accepts on this node.
    pub methods: Vec<ExecutionMethod>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LanguageRuntime {
    /// Value clients pass as `runtime` and the manifest's `runtime` field,
    /// e.g. "python3.13". Selects an install recipe + cache profile.
    pub id: String,
    /// Full version string, e.g. "3.13.1".
    pub version: String,
    /// Chosen when the client omits `runtime`.
    pub is_default: bool,
    /// True once this profile's install layers are materialized in the cache
    /// (a cold `runtime` still works on `next` but pays a materialize).
    pub prewarmed: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMethod {
    /// Pick language + runtime, send code; server desugars into a bundle.
    RawScript,
    /// Code-only tar over the minimal base rootfs; runtime provisioned by the
    /// startup script's cached install commands (or a static binary).
    Bundle,
    /// Self-contained OCI image bringing its own runtime/interpreter.
    /// See "Packaging: OCI bundles". Node-gated; may be curated in prod.
    OciBundle,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionModel { Deno, Gvisor }
```

Example served payload (a `next` node with Python raw-script + bundle, Rust
bundle-only, JS via Deno):

```json
{
  "languages": [
    { "name": "javascript", "display_name": "JavaScript",
      "execution_model": "deno", "runtimes": [], "methods": ["raw_script"] },
    { "name": "python", "display_name": "Python",
      "execution_model": "gvisor",
      "runtimes": [
        { "id": "python3.13", "version": "3.13.1", "is_default": true, "prewarmed": true },
        { "id": "python3.12", "version": "3.12.7", "is_default": false, "prewarmed": true }
      ],
      "methods": ["raw_script", "bundle"] },
    { "name": "rust", "display_name": "Rust (compiled binary)",
      "execution_model": "gvisor", "runtimes": [], "methods": ["bundle"] }
  ],
  "provisioning": {
    "model": "install_cache",
    "policy": "any",
    "package_source": "snapshot.debian.org@2026-06-01"
  },
  "oci_bundles": { "accepted": true, "policy": "any" }
}
```

- `provisioning` is a node-level block describing the install-cache model: `policy`
  is `"any"` on `next` (arbitrary live installs from the pinned source) vs.
  `"curated"` on prod (only pre-warmed profiles; no arbitrary installs), and
  `package_source` is the pinned mirror. Clients read it to know whether a bundle
  that installs an off-list package will be admitted.
- `oci_bundles` gates the escape-hatch path (not per-language, since the image
  carries its own runtime): `"any"` on `next` vs. `"curated"` (signed base-image
  digest allowlist) on prod.

## Runtime provisioning — minimal base image + content-addressed install cache

**Decision: the sandbox base image ships _no_ language runtimes.** It carries
only the bare essentials — a shell, coreutils, a package manager
(`apt`/`ca-certificates`), and the guest `lit` CLI. Every runtime, interpreter,
and package an action needs is **provisioned at run time by the startup script**
(`apt-get install python3.12`, `pip install …`), and the **result of each such
command is cached and keyed by the command**, so it is materialized once and
then reused — across executions and across *different* actions — as a fast
overlay mount.

This keeps the default TCB minimal (a Rust static binary pulls in nothing beyond
libc), makes language support a data/cache concern rather than an image-rebuild,
and lets `next` and prod differ purely by cache/policy rather than by shipping
different node images.

### The layer cache

The sandbox rootfs is composed of stacked overlay layers:

```
[ minimal base rootfs (RO) ]
  + [ layer: `apt-get install -y python3.12=3.12.7-1` ]   (cached, content-addressed)
  + [ layer: `pip install cryptography==43.0.1` ]         (cached, content-addressed)
  + [ /action bundle (RO) ]  + [ tmpfs /tmp, in-memory overlay for writes ]
```

- **Cache key** = `hash(parent_layer_id ‖ normalized_command ‖ resolved_inputs)`.
  Because the key includes the parent layer, `pip install X` on top of
  `python3.12` is a different (and correctly reused) entry than the same `pip`
  command on top of `python3.13`.
- **Cache value** is the filesystem diff the command produced (the overlay
  upperdir), stored **content-addressed by the hash of its bytes** — so identical
  results dedupe and a corrupt/poisoned entry can't be silently substituted.
- **Materialize (miss):** run the command once in a network-enabled materialize
  sandbox, snapshot the diff, store it. **Replay (hit):** mount the cached layer
  read-only; the script's install command then hits the package manager's
  "already satisfied" fast path and is effectively a no-op.

### Inspecting the startup script

Before execution the supervisor **parses the startup bash script**, extracts the
provisioning commands, and:

1. **Prefetch / warm** — resolve each command to a cache layer and compose the
   rootfs from hits; only genuinely-new commands hit the materialize path.
2. **Policy enforcement** — check each install against the environment's policy
   (allowed languages/packages/sources) *before* anything runs.

Static inspection drives prefetch and policy; **correctness still comes from the
overlay layers being present at run time** (transparent replay), so an install
line the parser didn't recognize still works — it just misses the warm path.

### Determinism (required for a sound cache and for TEE reproducibility)

`apt-get update && apt-get install python3.12` is only cacheable if the same
command yields the same bytes. Two requirements:

- **Pin versions** (`python3.12=3.12.7-1`, `cryptography==43.0.1`). Unpinned
  installs are treated as cache-miss-every-time (or rejected in prod).
- **Snapshot/pin the package source** — an internal mirror or `snapshot.debian.org`
  pinned by date/digest, so `apt-get update` resolves to a fixed index. The
  content-addressed value hash then detects any drift.

### Cold start & billing

First materialization of a language is slow (network + unpack); every reuse is a
cheap overlay mount. **Pre-warm** the common profiles (e.g. `python3.12`,
`python3.13`) at deploy time so the first real user request is already a hit.
Materialization time should be **excluded from billing** (extends the #557
cold-start-exclusion follow-up); reuse is on the hot path.

## Feature-gating across environments

With no runtimes baked into the image, gating is entirely **policy + cache**, in
two layers:

### Layer 1 — provisioning policy (what may be installed, from where)

What the materialize pass is allowed to install, set with the deploy config:

- **`next`** — permissive: any package from the pinned mirror, live materialize
  allowed. Fast iteration; new languages need no redeploy.
- **`prod`** — curated: **only pre-warmed cache profiles may be used and no
  arbitrary live install is permitted**; the package source is a locked internal
  mirror. A startup script asking for something outside the curated set is
  rejected at admission, not at run time.

This replaces the old "which runtimes are baked into the image" gate — the image
is the same minimal one everywhere; only the *policy and the warmed cache* differ.

### Layer 2 — language/method allowlist (deploy YAML)

Which `(language, method)` tuples the API admits, read by lit-api-server at
startup from `docker-compose.phala.yml`:

```yaml
  lit-api-server:
    environment:
      # (lang:runtime[:runtime...]|methods) — set by the deploy pipeline.
      LIT_SUPPORTED_LANGUAGES: ${LIT_SUPPORTED_LANGUAGES}
```

- `next`:   `javascript||raw_script; python:python3.13:python3.12|raw_script,bundle; rust||bundle`
- `prod`:   `javascript||raw_script; python:python3.13|bundle`

(Exact encoding TBD — a small TOML/JSON file mounted read-only is friendlier to
review; the point is it lives with the deploy config, not in the binary.)

> Note: changing `docker-compose.phala.yml` (env, mirror pin, warmed-cache set)
> changes `compose_hash`, a **governed change in production** (per #558). `next`
> can iterate freely.

### Startup self-check (drift guard)

On boot, lit-api-server confirms that every runtime it advertises for
`raw_script` has a **pre-warmed cache profile** available on the gVisor runner
(and, in prod, that the curated package source is reachable). If it advertises
`python3.13` raw-script but no `python3.13` profile is warmed, **fail fast at
startup** rather than paying a cold materialize on the first user request or
returning confusing errors. This keeps the advertised surface honest.

## Packaging options

Three ways to get a runtime in front of the action; all run under the same
`runsc` sandbox and op-loop. Ordered from lightest/default to escape-hatch.

### 1. Install-cache (default) — declare deps in the startup script

The bundle is **just code** (`tar`/`tar.gz` + `lit.json`); its startup script
provisions the runtime via cached install commands (above). Interpreted
languages (Python) and dynamically-linked binaries use this. Tiny bundles,
runtime shared via the cache across all actions.

### 2. Static binary in the code bundle — no runtime at all

Compiled languages (Rust, C++, Go) can ship a **static binary** (`musl`) in the
code bundle; it needs nothing installed, so no cache layer is involved. Smallest
TCB, fastest. Best for compiled actions whose deps close statically.

### 3. Self-contained OCI image — bring your own rootfs (escape hatch)

For a complex dependency closure or exact reproducibility, ship a **full OCI
image** (`oci-layout` archive, or a `docker-archive` tar) whose rootfs already
contains everything. The supervisor unpacks the layers into a per-execution
rootfs and runs *that* under `runsc` (already an OCI-compliant runtime) instead
of composing from the base + cache. Its layers can also **seed the shared layer
cache**, so a popular base image warms once and is reused.

- **+** Fully decoupled from node policy; reproducible/version-pinned; handles
  dep closures the install path can't.
- **-** Large bundles (tens–hundreds of MB) → slower cold start, subject to
  `max_code_length` (may need raising in chain config); larger in-sandbox TCB and
  a supply-chain surface (still sandboxed, **no keys**).

**Cross-cutting for the OCI path**

- **`lit` CLI injection.** Self-contained images won't contain `lit`; the
  supervisor **bind-mounts the static `lit` binary** (and the op socket at
  `/run/lit`) into the OCI rootfs, exactly as for the base rootfs. Don't require
  images to vendor `lit`.
- **`lit.json` still applies.** `entrypoint`/`env` select what runs (falling back
  to the image's own config). A `base` field distinguishes the models:
  ```json
  { "entrypoint": "run.sh" }                 // default: base + install-cache
  { "base": "oci" }                          // rootfs comes from the OCI image
  ```
- **Auth unchanged.** Checksum derived from the raw bundle bytes (the OCI archive
  here) → `ipfs_cid_to_u256` → `can_execute_action`.
- **Unpacking dependency.** Needs `umoci`/`skopeo`-style logic (or in-crate);
  scoped in Phase 6.
- **Gating.** OCI acceptance is a node capability, gated separately: `next`
  allows arbitrary images; **prod disallows or restricts to a curated set of
  signed base-image digests** (supply-chain control), advertised via discovery so
  clients don't upload an image the node will reject.

## The discovery endpoint

`GET /get_supported_languages`, tag `Configuration`, mirroring
`get_lit_action_client_config` exactly (no guards; it advertises capability).

```rust
// endpoints/configuration.rs
#[openapi(tag = "Configuration")]
#[get("/get_supported_languages")]
pub(super) async fn get_supported_languages(
    languages: &State<Arc<SupportedLanguages>>,   // parsed allowlist, managed state
) -> OpenApiResponse<SupportedLanguagesResponse, ErrMessage> {
    OpenApiResponse {
        response: ApiResult(Ok(languages.snapshot())).into(),
    }
}
```

- Register in `endpoints/mod.rs` `openapi_get_routes_spec!` alongside
  `get_lit_action_client_config`.
- `SupportedLanguages` is built once at startup from `LIT_SUPPORTED_LANGUAGES`,
  stored as Rocket managed state in `main.rs` (same shape as
  `LitActionsGvisorSocketPath`).
- Response type `SupportedLanguagesResponse { languages: Vec<LanguageFeature> }`
  in `models/response.rs`.
- **Regenerate `k6/litApiServer.ts`** after adding the route (CI k6-client-check
  gate — regenerate via `openapi_spec | @grafana/openapi-to-k6`).

Request admission reuses the same registry: `/lit_raw_action` rejects a
`(language, runtime)` not in the allowlist with a 4xx before any sandbox spins
up; `/lit_binary_action` validates the manifest's `runtime` against it too.

## Where the runtime version comes from

Because nothing is baked into the image, "Runtime version (multiple may exist)"
is expressed through the **install cache**, not a table in the rootfs:

- For a **bundle**, the startup script's own install commands are authoritative
  (`apt-get install python3.12=…`). The manifest `runtime` field is a hint used
  for prefetch/policy and for choosing the interpreter to invoke; it must be
  consistent with what the script installs.
- For **raw-script**, the client picks `runtime` (e.g. `python3.13`) and the
  server emits the matching **install recipe + entrypoint** from the language
  registry into the synthesized startup script — the client never writes the
  install line. The chosen runtime must be an allowlisted, pre-warmed profile.
- "Multiple versions coexist" falls out naturally: `python3.12` and `python3.13`
  are simply two different cache layers, each keyed by its own install command.

This is registry + install-recipe data in lit-api-server plus the cache
subsystem in `gvisor-server`; the wire contract is unchanged.

## Startup scripts & passing secrets

The unit of execution for non-trivial actions is a **bundle** whose entrypoint
is a small **startup shell script**. The script's job:

1. read run params via `lit params`,
2. fetch PKP-derived secrets via the `lit` CLI,
3. hand them to the app **without putting them in argv**,
4. run the app,
5. record the result via `lit set-response`.

Guest `lit` CLI surface available to these scripts (from #557):

```
lit job                         # full job JSON (params, authContext, headers, cid)
lit params                      # jsParams JSON
lit print "…"                   # append to action logs (stdout/stderr also captured)
lit set-response '…'            # record the response ("-" reads stdin)
lit get-private-key <pkpId>     # raw derived key for a permitted PKP wallet
lit get-action-private-key      # this action's own derived key
lit get-action-public-key [cid]
lit get-action-wallet-address [cid]
lit aes-encrypt <pkpId> <msg>   # / aes-decrypt  ("-" reads stdin)
lit increment-fetch-count
```

### Secret-handling rules (document these)

- **Never pass a secret as an argv token.** `/proc/<pid>/cmdline` is readable
  within the sandbox and argv has a size limit. Use **stdin** (preferred) or an
  **environment variable** (acceptable — the sandbox is single-tenant and
  per-execution, torn down after the run).
- Prefer `lit … -` / stdin for large payloads (argv limit).
- Keep secrets off `stdout`/`stderr` — both are forwarded into the action logs.
  Put only the intended response through `lit set-response`.
- The sandbox has **no persistence** and **no key material of its own**; every
  secret is derived on demand in the TEE and proxied in. Losing/leaking the
  in-sandbox copy reveals nothing after the run ends.

### Example 1 — Python (bundle with a startup script)

`lit.json`:

```json
{ "entrypoint": "run.sh", "runtime": "python3.12" }
```

(`runtime` matches the version `run.sh` installs and invokes below — the manifest
`runtime` is authoritative for prefetch/policy, so the two must agree.)

`run.sh` (the startup bash script — provisions the interpreter, fetches the key,
feeds the app via env + stdin, never argv):

```sh
#!/bin/sh
set -eu

# Provision the interpreter. Pinned version -> this command's result is cached
# and keyed by the command, so it's a fast overlay mount on every subsequent
# run (this action or any other). The pinned mirror makes it deterministic.
apt-get install -y --no-install-recommends python3.12=3.12.7-1

# Params for this run (JSON). Non-secret, so stdin to the app is fine.
PARAMS="$(lit params)"
PKP_ID="$(printf '%s' "$PARAMS" | jq -r '.pkpId')"

# Fetch the raw derived key for a permitted PKP. Keep it OUT of argv/logs:
# export it so the child inherits it via the environment.
LIT_PKP_SECRET="$(lit get-private-key "$PKP_ID")"
export LIT_PKP_SECRET

# Run the app. Key via env, params via stdin, result captured from stdout.
RESPONSE="$(printf '%s' "$PARAMS" | python3.12 app.py)"

# Record the response returned to the caller.
printf '%s' "$RESPONSE" | lit set-response -
```

> The first ever run of `python3.12=3.12.7-1` materializes the layer (network +
> unpack); every run after that — across all actions — reuses the cached layer.
> `pip install <pkg>==<ver>` lines work the same way, cached on top of the
> interpreter layer.

`app.py` (reads the secret from the environment, signs, prints only the result):

```python
import json, os, sys
# import your signing lib, e.g. eth_keys / solders — YOUR imported code signs
# inside the sandbox; the raw key never leaves it.
params = json.load(sys.stdin)
secret = os.environ["LIT_PKP_SECRET"]           # never printed
signature = f"signed:{params['message']}:{len(secret)}"  # placeholder
print(json.dumps({"ok": True, "signature": signature}))  # stdout -> response
```

For the **raw-script** path the same Python is submitted as bare `app.py`-style
source to `POST /lit_raw_action` with `{ "language": "python", "runtime":
"python3.13", "code": "…", "js_params": {…} }`; the server synthesizes the
`main.py` + `lit.json` wrapper and it runs identically. Raw scripts that only
need `lit get-private-key`/`lit aes-*` can call the CLI directly (see the #557
`examples/python/main.py`); the startup-script pattern above is for when you want
a shell to marshal secrets into a separate program.

### Example 2 — Rust (prebuilt binary bundle)

Compiled languages are bundle-only: build the binary **for the sandbox target**
(linux/amd64, static — `x86_64-unknown-linux-musl` recommended so it doesn't
depend on the rootfs's exact glibc), then ship it with a startup script. A
**static** binary needs no install layer at all (smallest TCB); a
dynamically-linked one adds a cached `apt-get install -y lib…=<ver>` line just
like the Python interpreter above.

`lit.json`:

```json
{ "entrypoint": "run.sh" }
```

`run.sh` (feeds the key to the binary on **stdin**, params via env):

```sh
#!/bin/sh
set -eu

PARAMS="$(lit params)"
PKP_ID="$(printf '%s' "$PARAMS" | jq -r '.pkpId')"

# Params are non-secret -> env is fine. The KEY goes on stdin so it never
# appears in argv or the process list.
export LIT_PARAMS="$PARAMS"
RESPONSE="$(lit get-private-key "$PKP_ID" | ./signer)"

printf '%s' "$RESPONSE" | lit set-response -
```

`signer` (Rust, sketch — reads key from stdin, params from env, prints result):

```rust
use std::io::Read;
fn main() {
    let mut key = String::new();
    std::io::stdin().read_to_string(&mut key).unwrap();     // raw derived key
    let params = std::env::var("LIT_PARAMS").unwrap_or_default();
    // sign with your crate (k256 / ed25519-dalek); key stays in-process.
    let sig = format!("signed:{}:{}", params.len(), key.trim().len());
    print!("{{\"ok\":true,\"signature\":\"{sig}\"}}");        // stdout -> response
}
```

Build + package + submit:

```sh
cargo build --release --target x86_64-unknown-linux-musl
cp target/x86_64-unknown-linux-musl/release/signer ./signer
tar czf bundle.tgz run.sh signer lit.json
# base64 the tar and POST it to /lit_binary_action:
BUNDLE="$(base64 -w0 bundle.tgz)"
curl -sX POST "$NODE/lit_binary_action" \
  -H "X-Api-Key: $API_KEY" -H 'Content-Type: application/json' \
  -d "{\"bundle\":\"$BUNDLE\",\"js_params\":{\"pkpId\":\"$PKP_ID\",\"message\":\"hi\"}}"
```

Both examples also work with **multiple PKPs** — call `lit get-private-key`
once per `pkpId` the action is permitted to use.

## Delivery plan

Each phase is a separate PR; **Done = lands on `next` with green CI**. Phases 1–3
stack on #557/#558.

### Phase 0 — Prereqs
- [ ] #557, #558, #559 merged to `next`; `lit-actions-gvisor` service live on a
      `next` CVM and `/lit_binary_action` exercised end-to-end.

### Phase 1 — Language registry + discovery endpoint
- [ ] `actions/languages.rs`: `LanguageFeature`/`LanguageRuntime`/`ExecutionMethod`/`ExecutionModel`, allowlist parser.
- [ ] `LIT_SUPPORTED_LANGUAGES` parsed at startup → `SupportedLanguages` managed state (`main.rs`).
- [ ] `GET /get_supported_languages` (`endpoints/configuration.rs`, registered in `mod.rs`).
- [ ] `SupportedLanguagesResponse` in `models/response.rs`.
- [ ] Regenerate `k6/litApiServer.ts` (k6-client-check gate).
- [ ] Unit tests for the allowlist parser (round-trip, unknown tokens rejected).
- [ ] Docs: `docs/lit-actions/languages.mdx` (capability discovery).

### Phase 2 — Content-addressed install cache (the foundation for all non-JS)
- [ ] Strip runtimes from `image/Dockerfile.rootfs` — base ships only shell, coreutils, `apt` + `ca-certificates`, and the `lit` CLI (minimal TCB).
- [ ] Overlay layer-cache in `gvisor-server`: rootfs = base + stacked install layers + `/action`; key = `hash(parent ‖ normalized_cmd ‖ resolved_inputs)`; value content-addressed by produced-file hash, integrity-checked before mount.
- [ ] Materialize path: run an uncached command once in a network-enabled sandbox, snapshot the overlay diff, store it; replay path mounts hits read-only.
- [ ] Startup-script inspection: extract provisioning commands for prefetch + policy (correctness via transparent replay, not parsing).
- [ ] Determinism: pinned package source (mirror/`snapshot.debian.org`) + version-pinned installs; unpinned = miss-every-time on `next`, rejected on prod.
- [ ] Pre-warm the default profiles at deploy time; exclude materialize time from billing.
- [ ] Integration test: two actions sharing a `python3.12` install → second run is a cache hit (no network).

### Phase 3 — Raw-script path for Python
- [ ] `POST /lit_raw_action` with `LitRawActionRequest { language, runtime?, code, js_params }`.
- [ ] Server-side desugar: synthesize `main.<ext>` + `run.sh` (with the registry's install recipe for the chosen runtime) + `lit.json`, route through the `/lit_binary_action` code path (derive checksum → authorize → run). JS `language` short-circuits to the Deno runner / existing `/lit_action` path.
- [ ] Reject `(language, runtime, raw_script)` not in the allowlist, and a `runtime` with no pre-warmed profile on prod, before sandbox spin-up.
- [ ] Regenerate `k6/litApiServer.ts`.
- [ ] Integration test: Python raw script round-trips (params in, `set-response` out) via the process runtime.

### Phase 4 — Startup-script conventions + examples + docs
- [ ] Ship the Python and Rust startup-script examples under `lit-actions/gvisor-server/examples/` (alongside the existing `python/` and `shell/`).
- [ ] `docs/lit-actions/` page: bundle format, `lit` CLI reference, secret-handling rules, the two worked examples, and the build-for-target (musl) guidance for compiled languages.
- [ ] Cross-link from `docs/lit-actions/secrets.mdx` and `index.mdx`.

### Phase 5 — Deploy enablement
- [ ] Set `LIT_SUPPORTED_LANGUAGES` + provisioning policy (`any` on `next`, `curated` on prod) + pinned package source in the deploy pipeline.
- [ ] Pre-warm the curated cache profiles for prod; publish the single minimal rootfs digest (same image everywhere).
- [ ] Governed `compose_hash` change for prod once soaked on `next`.

### Phase 6 — OCI bundles (with a bundled runtime)
- [ ] Bundle-type detection: OCI archive (`oci-layout` / `docker-archive`) vs. code tar; `lit.json` `base: "oci"`.
- [ ] Supervisor: unpack image layers into a per-exec rootfs (`umoci`/`skopeo`-style or in-crate), generate the OCI `config.json`, run under `runsc` — reusing the existing mount/cgroup/host-uds setup.
- [ ] Bind-mount the static `lit` binary + op socket into the OCI rootfs (don't require images to vendor `lit`).
- [ ] Entrypoint resolution: `lit.json` `entrypoint` wins, else the image's own config.
- [ ] `oci_bundle` method + node `oci_bundles` capability in the discovery payload and admission checks; `policy: curated` = signed base-image digest allowlist for prod.
- [ ] Integration test: a self-contained Python OCI image (interpreter not in the base rootfs) round-trips secrets via `lit`.
- [ ] Docs: when to reach for OCI vs. code bundle; size/cold-start tradeoffs; base-image provenance.

## Security considerations

- **Trust boundary is unchanged.** Guest code (any language) runs in a `runsc`
  sandbox with no key material; every secret is derived in the TEE and proxied
  over a **per-execution** op socket (`--host-uds=all` on a fresh tempdir).
- **Authorization is on the derived checksum**, never a client-supplied value
  (`checksum → ipfs_cid_to_u256 → can_execute_action`, per #558). Raw-script
  desugaring must derive the checksum from the *synthesized tar bytes* so the
  same rule applies.
- **Install-cache integrity.** Every layer is content-addressed by the hash of
  its produced bytes and integrity-checked before mount, so a poisoned or drifted
  entry can't be silently substituted. Cache is keyed with the parent layer, so
  the same command on different bases can't collide.
- **Materialize-pass egress.** Live installs fetch from the network into the
  enclave — restrict egress to the pinned package source only; verify apt GPG /
  pip hashes. Prod uses **curated, pre-warmed profiles with no arbitrary live
  install**, so the attested runtime set is fixed and reproducible.
- **Determinism for attestation.** Version-pinned installs + a snapshot-pinned
  mirror mean a given startup script resolves to the same layers every time —
  required if runtime provenance is ever attested.
- **Larger TCB for interpreted languages.** A provisioned CPython enlarges attack
  surface vs. bare JS, but only *for actions that install it* — the base image
  stays minimal, and static-binary actions add nothing. Prefer static/compiled
  where the binary is the only added surface.
- **Bundle size.** Bundles are subject to `max_code_length`; large compiled
  binaries — and especially OCI images — may need it raised in chain config
  (noted in #558).
- **OCI supply chain.** A self-contained OCI image means running third-party
  layers: it stays sandboxed with no keys, but on prod restrict acceptance to a
  curated allowlist of signed base-image digests (`oci_bundles.policy =
  curated`), and always inject `lit` from the host rather than trusting an
  image-vendored copy.
- **Secret hygiene in scripts.** Enforced by convention + docs: stdin/env only,
  never argv; secrets never on stdout/stderr (both are logged).

## Open questions

1. **Allowlist encoding** — compact env string vs. a mounted `languages.toml`.
   The file is more reviewable and diffs cleanly under `compose_hash` governance;
   the env string is simpler to template. Lean file.
2. **Raw-script endpoint shape** — new `/lit_raw_action` (recommended, keeps
   `/lit_action` JS-pure) vs. extending `LitActionRequest` with optional
   `language`/`runtime`. New endpoint avoids overloading the hot JS path.
3. **Compilation service?** — out of scope, but a future "send Rust source, we
   build it" service would need a separate, untrusted build tier; explicitly not
   in the enclave.
4. **Per-runtime limits** — should timeout/memory defaults differ by language
   (Python needs more than a static binary)? Start uniform (#557 defaults),
   revisit with the perf gate.
5. **Billing** — raw-script Python and bundles both bill flat per-second today.
   Confirm product wants parity with JS pricing before GA.
6. **OCI archive format** — accept `oci-layout` archives, `docker-archive`
   (`docker save`) tars, or both? And in-crate layer unpack vs. shelling to
   `umoci`/`skopeo` inside the supervisor. `oci-layout` is the more standard,
   registry-native choice.
7. **OCI cache + eviction** — code-bundle caching is keyed by CID and currently
   unbounded (#557 follow-up); OCI images and install layers are far larger, so
   eviction/quota (LRU by layer, size ceiling) becomes mandatory rather than a
   follow-up.
8. **Install-command recognition** — how much to parse the startup script for
   prefetch/policy. A declared `deps` array in `lit.json` (explicit, easy to gate)
   vs. best-effort parsing of `apt-get`/`pip` lines (zero author friction, but
   fragile). Likely both: parse for warmth, but let `lit.json` declare for policy.
9. **`apt-get update` determinism** — pin the mirror by snapshot date/digest, or
   ship a pre-populated apt index in the base? Snapshot mirror is simplest and
   keeps the base image tiny.
