# Multi-language Lit Actions — what matters to a client

Companion to [`multi-language-lit-actions.md`](./multi-language-lit-actions.md).
That doc is the build plan; this one strips it down to the parts a **client**
(someone writing and submitting a Lit Action) actually has to decide or design
around. Everything else in the plan is node/platform machinery the client never
touches.

For each step: **Choice** = the client picks it; **Fixed** = the node dictates
it and the client must discover/comply; **Matters** = why it changes what you
build.

## Decision flow

```
1. Discover node capabilities   → GET /get_supported_languages
2. Pick an execution method     → raw-script | bundle | OCI image
3. Pick language + runtime      → must be advertised; pin the version
4. (bundle) Write startup script→ provision deps, marshal secrets, run, respond
5. Package + submit             → /lit_action | /lit_raw_action | /lit_binary_action
                                  (repeat runs: re-invoke by checksum, not bytes)
```

Speed is cross-cutting — pinned versions, prewarmed runtimes, and bundle-by-CID
reuse decide whether a run is a warm mount or a cold materialize (see §5).

## 1. Discover what the node supports — don't hardcode

**Fixed (discover it):** call `GET /get_supported_languages` first. Capabilities
differ by environment — an internal/`next` node exposes more than production.

| Field | What it tells the client | Why it matters |
|---|---|---|
| `languages[].name` + `methods` | which languages exist and which methods each accepts | a language may be bundle-only (e.g. Rust) |
| `runtimes[].id` / `version` / `is_default` | exact runtime versions you may request | pick one; don't assume "python3" |
| `runtimes[].prewarmed` | whether that runtime is already cached | non-prewarmed = slow first run (cold materialize) |
| `provisioning.policy` | `any` (arbitrary installs allowed) vs `curated` (pre-warmed only) | on prod, an off-list `apt/pip` install is **rejected** |
| `provisioning.package_source` | the pinned mirror your installs resolve against | pin versions available there |
| `oci_bundles.accepted` / `policy` | whether custom OCI images run, and if curated | if curated, only signed base-image digests run |

## 2. Choose an execution method — the primary decision

| Method | Endpoint | You send | Pick it when | Cost/tradeoff |
|---|---|---|---|---|
| **Raw script** | `/lit_action` (JS), `/lit_raw_action` (others) | `language` + `runtime` + **code** | quick logic, no extra deps; JS or plain Python | least control; server writes the wrapper/install line for you |
| **Bundle** | `/lit_binary_action` | tar/tar.gz project + `lit.json` + startup script | multiple files, pip deps, a compiled binary, custom provisioning | you own the startup script + packaging |
| **OCI image** | `/lit_binary_action` (`base: "oci"`) | a full OCI image | complex dep closures, exact reproducibility, dynamic linking | tens–hundreds of MB; slower cold start; may be disallowed/curated on prod |

Rules of thumb:
- **Interpreted (Python):** raw-script for simple, bundle when you need deps/files.
- **Compiled (Rust/Go/C++):** bundle-only — ship a prebuilt (ideally static
  `musl`) binary; there is no "send source" path (no compiler in the enclave).
- **JavaScript:** stays on the fast dedicated `/lit_action` (Deno) path.

## 3. Choose language + runtime version

- **Choice:** which `(language, runtime)` — but only from what step 1 advertised.
- **Matters — pin the version.** `python3.12=3.12.7-1`, `pip install x==1.2.3`.
  Pinned installs hit the shared cache (fast) and are deterministic; unpinned
  ones are slow every time on `next` and **rejected on prod**.
- **Matters — prefer `prewarmed` runtimes** for predictable latency.

## 4. Provisioning deps (bundle path) — your startup script's install lines

The base sandbox ships **no language runtimes**; your startup script installs
what it needs, and each install command's result is **cached and reused across
runs and across actions** (keyed by the command).

- **Choice:** what to install (`apt-get install …`, `pip install …`).
- **Matters:** pin versions (cache + determinism); the **first** run of a new
  install is slow (network + unpack), every later run is a fast cache hit.
- **Fixed:** on prod (`policy: curated`) you can only use pre-warmed profiles —
  arbitrary installs are refused. Check `provisioning.policy` before relying on
  a custom package.
- **Static compiled binary** needs no install line at all (smallest, fastest).

## 5. Speed & caching — how to make your action fast

Two things dominate latency: **transferring/unpacking the bundle** and
**provisioning the runtime**. Both are cached, so the *second* run of a given
action is far faster than the first. These are the client-side levers:

| Lever | Do this | Impact |
|---|---|---|
| **Reuse the bundle by CID** | after the first submit, re-invoke with `checksum` only (no `bundle` bytes) — the runner still has it cached | skips re-upload **and** re-unpack |
| **Deterministic packaging** | build a reproducible tar (sorted entries, fixed mtimes) | identical bytes → same checksum → cache hit instead of a "new" bundle |
| **Pick a `prewarmed` runtime** | choose a runtime the node reports `prewarmed: true` | avoids the cold materialize (network + unpack) on first run |
| **Pin + share install commands** | pin versions and use the *same* install lines other actions use | shared content-addressed layers are mounted, not re-materialized |
| **Order install layers shared-first** | install the interpreter first, action-specific `pip`/`apt` deps last | the cache key chains on the parent, so a shared prefix is reused across actions |
| **Minimize deps** | install only what you use | fewer/smaller layers to mount |
| **Static binary** | ship a static `musl` binary with no install line | zero install layers → fastest cold start |
| **Avoid unpinned installs** | never `apt-get install python3` without a version | miss-every-time on `next`; rejected on prod |
| **Reserve OCI for when needed** | don't use a large OCI image on a hot path | big unpack = slow cold start; prefer base + install-cache |
| **Pure JS → `/lit_action`** | keep JS-only logic on the Deno path | no sandbox spin-up / bundle unpack at all |

**Cold vs. warm:** the first execution of a novel `(bundle, install-set)` pays
the full materialize (network + unpack); steady-state runs are warm mounts.
Materialize time is **not billed**, but it *is* latency — if you're
latency-sensitive, exercise the action once to warm the caches before going
live. What's warm is node-local, so a fresh/scaled node starts cold.

## 6. The startup script contract

- **Choice:** the `entrypoint` in `lit.json` (an argv array, or a string run as
  `sh <file>` — no exec bit needed).
- **Fixed semantics:** the run **ends when the entrypoint exits**. Exit `0` =
  success with the last `lit set-response`; non-zero = failure with a stderr
  tail. No response set = empty response.
- **Fixed:** the guest `lit` CLI is preinstalled and is your only channel to the
  platform (params, secrets, response, logging).

## 7. Passing secrets / using PKPs — the part to get right

Secrets (PKP-derived keys) are fetched inside the sandbox via `lit` and fed to
your app. The key **is never in your submitted code** — it's derived in the TEE
and proxied in at run time.

`lit` ops a client uses:

```
lit params                    # your jsParams JSON
lit get-private-key <pkpId>   # raw derived key for a permitted PKP  (repeat per PKP)
lit get-action-private-key    # this action's own derived key
lit aes-encrypt <pkpId> <msg> # / aes-decrypt   ("-" reads stdin)
lit set-response '…'          # record the response  ("-" reads stdin)
lit print "…"                 # append to logs
```

**Matters (security rules you must follow):**
- **Never** pass a secret as an argv token — `/proc/<pid>/cmdline` is readable in
  the sandbox. Use **stdin** (preferred) or an **env var**.
- Keep secrets **off stdout/stderr** — both are captured into the action logs.
  Only your intended result should go through `lit set-response`.
- **Multiple PKPs:** call `lit get-private-key` once per `pkpId` the action is
  permitted to use.

## 8. Limits you must design around (Fixed)

| Limit | Default | Max |
|---|---|---|
| timeout | 15 min | 150 min |
| memory | 512 MB | 2 GB |
| pids | 128 | — |
| tmpfs (`/tmp`) | 512 MB | — |
| bundle size | bounded by `max_code_length` (raise via chain config for large binaries/OCI) |

Network egress is proxied/metered; count HTTP calls with `lit increment-fetch-count`.

## 9. Client controls vs. platform controls

| Client controls | Platform / node controls |
|---|---|
| execution method, language, runtime version (from the advertised set) | which languages/runtimes/methods exist at all |
| bundle contents, entrypoint, install commands | whether arbitrary installs are allowed (`any` vs `curated`) |
| how secrets are marshaled into the app | key derivation, the trust boundary, per-exec isolation |
| the response and what gets logged | resource limits, billing, the pinned package source |

## 10. Pre-submit checklist

- [ ] Called `/get_supported_languages`; my language/runtime/method is in it.
- [ ] Runtime version is **pinned** and (ideally) `prewarmed`.
- [ ] If I install packages: versions pinned, and allowed under `provisioning.policy`.
- [ ] Startup script exits non-zero on failure; success path calls `lit set-response`.
- [ ] Secrets go via stdin/env, never argv; never printed to stdout/stderr.
- [ ] Compiled binary built for linux/amd64 (static `musl` unless I bring its libs).
- [ ] Bundle fits within `max_code_length`.
- [ ] For repeat runs, re-invoke by `checksum` instead of resending the bundle.
- [ ] Latency-sensitive? Warmed the caches with a throwaway run first.
