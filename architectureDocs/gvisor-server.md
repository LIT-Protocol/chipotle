# gVisor Server — Any-Language Lit Action Runner

## Core Insight

The gVisor server (`lit-actions/gvisor-server`, crate `lit-actions-gvisor-server`,
binary `lit_actions_gvisor`) lets a Lit Action be written in **any language** —
Python, shell, a compiled binary — instead of only JavaScript. It achieves this
without touching the protocol, the JS runner, or the security model, by a single
design decision:

**It speaks the *exact same* gRPC op-loop as the JS runner, on its own Unix
socket.** From lit-api-server's point of view there is no new runner — it is the
same `Action.ExecuteJs` bidirectional stream, the same op handlers, the same
billing path. The only things that differ are (1) the socket the client dials,
(2) the shape of the `code` payload (a content-addressed *bundle* instead of JS
source), and (3) the sandbox that runs it (a per-execution gVisor `runsc`
sandbox instead of a Deno/V8 isolate).

Because the two runners share the proto verbatim — the guest-ops messages are
`extern_path`'d to the op-loop proto types at build time — **the two interfaces
cannot drift apart**. Any op the JS runner supports, the gVisor runner supports,
because they are literally the same messages handled by the same lit-api-server
code.

> Source PRs:
> [#557](https://github.com/LIT-Protocol/chipotle/pull/557) (the `gvisor-server`
> crate) and
> [#558](https://github.com/LIT-Protocol/chipotle/pull/558) (the
> `/lit_binary_action` wiring in lit-api-server).

> **Update (2026-07, CPL-355):** the manifest `entrypoint` model described in
> this snapshot has been replaced. The sandbox now only ever executes
> **`bash startup.sh`** — supplied per-request via
> `ExecutionRequest.startup_script` / the endpoint's `startup_script` field
> (mounted read-only at `/startup/startup.sh`), falling back to a `startup.sh`
> at the bundle root. Bundles are pure payload (`lit.json` is optional, its
> `entrypoint` ignored), so one cached bundle serves many different startup
> scripts with full cache hits, and top-level js-params are injected into the
> sandbox environment. See `lit-actions/gvisor-server/README.md` for the
> current contract; entrypoint references below are historical.

---

## Where it sits

```
┌─────────────────────────────────────────────────────────────────────┐
│ CVM (Phala TDX) — one attested identity, holds root/signer keys       │
│                                                                       │
│   client ── HTTPS ──▶ lit-api-server (Rocket)                         │
│                          │                                            │
│              ┌───────────┴───────────┐                               │
│              │                        │                               │
│      POST /lit_action          POST /lit_binary_action               │
│              │                        │                               │
│              ▼                        ▼                               │
│   Client{socket=JS sock}     Client{socket=gVisor sock}              │
│              │                        │   (identical Client type,     │
│              │                        │    identical op-loop code —    │
│              │                        │    only socket_path differs)   │
│              │  Action.ExecuteJs      │  Action.ExecuteJs             │
│              │  (bidi gRPC/UDS)       │  (bidi gRPC/UDS)              │
│              ▼                        ▼                               │
│   /tmp/lit_actions.sock      /tmp/lit_actions_gvisor.sock            │
│              │                        │                               │
│              ▼                        ▼                               │
│   lit-actions (JS runner)    lit_actions_gvisor (this crate)          │
│     Deno / V8 isolate          Supervisor + sandbox runtime           │
│                                       │                               │
│                                       ▼  one runsc sandbox per exec    │
│                                ┌─────────────────────────────┐        │
│                                │ gVisor (runsc) sandbox        │       │
│                                │  • RO base rootfs + memory    │       │
│                                │    overlay (--overlay2)       │       │
│                                │  • bundle RO-mounted /action  │       │
│                                │  • tmpfs /tmp                 │       │
│                                │  • op socket at /run/lit/     │       │
│                                │    ops.sock (--host-uds=all)  │       │
│                                │  • preinstalled `lit` CLI     │       │
│                                │  • entrypoint = user code     │       │
│                                └─────────────────────────────┘        │
└─────────────────────────────────────────────────────────────────────┘
```

Both runners share the same `lit-socket` volume (`/tmp` in compose) and the same
attested CVM identity, so adding the gVisor runner introduces **no new cross-VM
trust plumbing** — it is a second process next to the JS runner.

---

## The wire contract (unchanged)

The gVisor server implements the `Action` gRPC service from
`lit-actions/grpc/schema/lit_actions.proto` — the **same service the JS runner
serves**. Its `execute_js` handler (`src/server.rs`) is a structural clone of
the JS runner's: rendezvous (zero-capacity) channels give strict one-op-in-flight
ordering, the first stream message must be the `Execute` request, an all-whitespace
`code` is the empty-action shortcut, and the final message is either a
`tonic::Status` error (timeout/cancel) or a success/failure `ExecutionResult`.

How the existing `ExecutionRequest` fields are reused for a bundle:

| Field | JS runner meaning | gVisor runner meaning |
| --- | --- | --- |
| `code` | JS source string | **base64 tar/tar.gz bundle**, or `cid:<id>` referencing an already-cached bundle |
| `ipfs_id` | server-derived CID of the JS source | server-derived CID of the **raw tar bytes**; also the bundle-cache key |
| `js_params` | params exposed to the action | same — exposed to guest via `lit params` |
| `auth_context` | auth context JSON | same — exposed via `lit auth-context` |
| `http_headers` | request headers | same (incl. `x-request-id`, privacy-mode tag) |
| `timeout` / `memory_limit` | execution limits | same (clamped by the supervisor) |

Guest-callable ops (`Print`, `SetResponse`, `GetPrivateKey`,
`GetLitActionPrivateKey`, `GetLitActionPublicKey`, `GetLitActionWalletAddress`,
`AesEncrypt`, `AesDecrypt`, `IncrementFetchCount`) are proxied **1:1** from guest
code onto the op-loop. `UpdateResourceUsage` is also carried on the op-loop, but
it is emitted by the *supervisor* (usage ticks) and is deliberately absent from
`GuestOps` — guest code can never issue it (see the lifecycle section below).
Every op that touches a secret (key derivation, AES) is executed **by
lit-api-server**, never inside the sandbox — the sandbox only asks.

---

## Crate internals

The crate is a thin, well-separated pipeline. Shared guest-visible constants
(`GUEST_ACTION_DIR=/action`, `GUEST_SOCK_DIR=/run/lit`, `OP_SOCK_FILE=ops.sock`,
`LIT_OP_SOCK`, `LIT_ACTION_IPFS_ID`) live in `sandbox/mod.rs` so the runtimes,
the supervisor, the `lit` CLI, and the docs cannot drift.

| Module | Responsibility |
| --- | --- |
| `server.rs` | `Action` gRPC front-end. Same protocol surface as the JS runner; drives the op-loop stream and hands the first request to the supervisor. |
| `supervisor.rs` | One execution end-to-end: resolve bundle → bind per-exec op socket → spawn sandbox → wait (timeout / 500 ms usage ticks / fatal op errors / log forwarding) → teardown. Mirrors the JS runner's `execute_with_worker`. |
| `bridge.rs` | `OpBridge` — serializes concurrent op producers (guest RPCs, usage ticks, stdout/stderr forwarding) onto the single-op-in-flight op-loop with a tokio `Mutex`. Surfaces `ReportError` as an `Err`. |
| `guest_service.rs` | Per-execution `GuestOps` gRPC service on the sandbox's op socket. Every handler is a one-line forward through `OpBridge`. Also serves `GetJob`. |
| `proto.rs` / `schema/lit_guest.proto` | The `GuestOps` service + `Job` envelope. Op payload types are `extern_path`'d to the op-loop proto, so guest ops == op-loop ops. |
| `bundle.rs` | Content-addressed bundle cache — the any-language analog of the JS `ActionCodeCache`. Decodes/unpacks tar(.gz), validates the `lit.json` manifest, hardens against hostile archives. |
| `sandbox/mod.rs` | `SandboxRuntime` trait + `ExecSpec`. The abstraction both runtimes implement. |
| `sandbox/runsc.rs` | **Production** runtime: builds the OCI spec, one `runsc` sandbox per execution, `runsc kill` / `delete` teardown. |
| `sandbox/process.rs` | **Dev/CI only** runtime: runs the entrypoint as a plain child process (NO isolation). Same guest contract, so a bundle behaves identically. |
| `bin/lit.rs` | The guest `lit` CLI shipped in the base image. |
| `main.rs` | CLI/env parsing, runtime selection, server bootstrap. |

### The op bridge — why it exists

The op-loop allows exactly **one op in flight**: the runner sends an
`ExecuteJsResponse` (op request) and lit-api-server replies with exactly one
`ExecuteJsRequest` (the op response, or `ReportError`). The JS runner gets this
ordering for free from V8's single thread. Here the producers are genuinely
concurrent — the guest may call `lit` from multiple threads, the supervisor
emits usage ticks every 500 ms, and stdout/stderr are forwarded as `Print` ops.
`OpBridge::round_trip` holds a mutex across each send+recv pair so producers can
never interleave. This is the crux that lets an untrusted multi-threaded guest
share a strictly-sequential op stream.

---

## Request lifecycle (end-to-end)

```
client                lit-api-server                 lit_actions_gvisor              runsc sandbox
  │                        │                                 │                            │
  │ POST /lit_binary_action│                                 │                            │
  │  {bundle|checksum,      │                                 │                            │
  │   js_params}           │                                 │                            │
  ├───────────────────────▶│                                 │                            │
  │           resolve_binary_bundle():                       │                            │
  │           • decode base64 (whitespace-tolerant)          │                            │
  │           • checksum = IPFS-hash(raw tar bytes)          │                            │
  │           • authorize on DERIVED checksum                │                            │
  │             (ipfs_cid_to_u256 → can_execute_action)      │                            │
  │           build Client{ socket_path = gVisor socket }    │                            │
  │                        │  Action.ExecuteJs (bidi/UDS)    │                            │
  │                        ├────────────────────────────────▶│                            │
  │                        │  ExecutionRequest{code,ipfs_id, │                            │
  │                        │   js_params,timeout,…}          │                            │
  │                        │                    BundleCache.resolve():                    │
  │                        │                     unpack tar → /cache/<cid>                 │
  │                        │                    bind per-exec op socket, serve GuestOps    │
  │                        │                                 │  runsc run (OCI spec)      │
  │                        │                                 ├───────────────────────────▶│
  │                        │                                 │       entrypoint runs      │
  │                        │                                 │       (any language)       │
  │                        │                                 │◀── lit get-private-key ────┤
  │                        │◀── GetPrivateKey op ────────────┤   (over /run/lit/ops.sock) │
  │        (key derived in-TEE, returned)  ─────────────────▶│──── secret ───────────────▶│
  │                        │◀── UpdateResourceUsage (500ms) ─┤                            │
  │      bill wall-clock; may set cancel_action ────────────▶│                            │
  │                        │◀── Print(stdout/stderr) ────────┤                            │
  │                        │                                 │   entrypoint exits 0/n     │
  │                        │◀── ExecutionResult(success) ────┤◀───────────────────────────┤
  │◀── {response,logs} ────┤                                 │   teardown: kill+delete    │
```

Key semantics (all mirror the JS runner exactly):

- **Termination = the entrypoint exits** (serverless/CLI semantics). Exit `0` →
  success with the last-recorded response; non-zero → failure with a stderr
  tail. `lit set-response` only *records* the response server-side.
- **Timeout** → `DeadlineExceeded`; **billing cancel** (a usage tick returns
  `cancel_action`) → `ResourceExhausted`. Same status codes and messages as JS.
- **Usage ticks** every 500 ms come from the *supervisor*, never from guest
  code (the `UpdateResourceUsage` op is deliberately absent from `GuestOps`).
  v1 bills flat wall-clock per second; `used_kb` is `0` for now.
- **Log forwarding** reads raw bytes (not lines) so the pipe always drains,
  decodes lossily, chunks long lines (64 KB), and keeps an 8 KB stderr tail. A
  `Print` rejected by lit-api-server (log quota / `ReportError`) is fatal —
  parity with a throwing `console.log`.

---

## Relationship to lit-api-server (PR #558)

PR #558 is described as "purely the lit-api-server socket wiring" follow-up
from #557 — **no protocol, op-handler, or runtime changes**. The changes:

- **`actions/client/execution.rs`** — the enabler. The `Client` now honors its
  `socket_path` field (previously hardcoded to the JS socket). The JS path
  leaves it unset and falls back to `/tmp/lit_actions.sock`; the binary path
  sets it to the gVisor socket. `GrpcClientPool` pools connections keyed by
  socket path, so one pool serves both runners.
- **`core/v1/endpoints/actions.rs`** — new `/lit_binary_action` handler. Same
  guards as `/lit_action` (`CpuAvailable`, `BilledLitActionApiKey`), same
  response type (`LitActionResponse`).
- **`core/v1/models/request.rs`** — `LitBinaryActionRequest { bundle?,
  checksum?, js_params? }`. Provide `bundle` (base64 tar/tar.gz) or `checksum`
  (CID of an already-cached bundle).
- **`core/core_features.rs`** — `lit_binary_action()` + `resolve_binary_bundle()`.
  This is the security-critical part (below).
- **`core/v1/health.rs`** — `/health` also probes the gVisor socket
  (`lit_actions_gvisor_reachable`), but **informational only** — it does *not*
  gate health, so nodes stay healthy before the runner is rolled out. The probe
  logic was refactored into a shared `probe_socket()` used for both sockets.
- **`main.rs`** — `LitActionsGvisorSocketPath` managed state + the
  `LIT_ACTIONS_GVISOR_SOCKET` env override (default
  `/tmp/lit_actions_gvisor.sock`).
- **`docker-compose.phala.yml`** — the `lit-actions-gvisor` service; api-server
  `depends_on` it. Adding the service changes `compose_hash` (governed in prod).
- **`Cargo.toml` / `k6/litApiServer.ts`** — `base64 = "0.22"` (matched to the
  gVisor crate); regenerated k6 client with the new route.

### Authorization — derived, never client-supplied

The permission model is unchanged from JS actions. On-chain, an action is
authorized by the keccak hash of its content id; `can_execute_action` doesn't
care whether those bytes are a JS-source CID or a bundle CID.

`resolve_binary_bundle()` enforces the invariant that **you can only run bytes
you actually submitted**:

- With `bundle`: decode the base64 *exactly as the runner will* (strip
  whitespace, STANDARD alphabet), then `checksum = IPFS-hash(raw tar bytes)`.
  Authorization runs on this **derived** checksum. A client-supplied `checksum`
  is only a hint — if it disagrees, it is logged and ignored. So a caller can
  never authorize CID A and execute bytes B.
- With only `checksum`: emit `cid:<checksum>` (the runner resolves it from its
  own bundle cache) and authorize on that checksum — it maps back to the exact
  bytes that hashed to it.

The same `IpfsHasher` is used here and on-chain at registration, so a bundle
registered on-chain matches the checksum derived at execution time. Note the
two caches are distinct: **lit-api-server needs no server-side code cache for
binary actions — the gVisor runner owns the bundle cache.**

---

## Bundle format & cache

A bundle is a `tar` or `tar.gz` archive with a `lit.json` manifest at its root:

```json
{
  "entrypoint": ["python3", "main.py"],
  "runtime": "python3",
  "env": { "PYTHONUNBUFFERED": "1" }
}
```

- `entrypoint` — an argv array run verbatim, **or** a string treated as a shell
  script path (run as `sh <path>`; no exec bit needed).
- `runtime` — informational for now (the v1 base image ships all supported
  runtimes).
- `env` — optional extra environment variables.

`BundleCache` (`bundle.rs`) resolves `code` to an unpacked directory:

- `cid:<id>` → must already be cached (else "resend the bundle bytes").
- base64 payload → decode, derive the CID (server `ipfs_id` if supplied, else a
  local `sha256-<hex>` pseudo-id for tests), unpack once, cache by CID.
- Unpack is **atomic**: unpack into a temp sibling then `rename`, so a
  concurrent resolve of the same CID either wins the rename or finds the
  winner's directory — never a half-unpacked one. The unpacked dir is
  **read-only and shared** across executions; per-run writes go to the
  sandbox's tmpfs.

Hostile-archive hardening: only regular files and directories are allowed
(symlinks/devices/FIFOs rejected — they could alias outside the bundle);
setuid/setgid/sticky bits are stripped (rwx kept for binary entrypoints);
`unpack_in` refuses path traversal; caps of 256 MB unpacked / 10 000 entries
guard against zip bombs; the manifest is validated *before* the bundle is
published under its CID.

---

## Guest `lit` CLI

Preinstalled in the base image (`bin/lit.rs`), the `lit` CLI is how action code
in any language reaches the op-loop. It connects to `$LIT_OP_SOCK` (default
`/run/lit/ops.sock`) and translates each subcommand into one `GuestOps` RPC.
Alternatively a language SDK can link `schema/lit_guest.proto` directly for the
same surface.

```sh
lit job                          # full job JSON (params, authContext, headers, cid)
lit params                       # jsParams JSON
lit auth-context                 # authContext JSON
lit print "hello"                # append to action logs
lit set-response '{"ok":true}'   # record the response ("-" reads stdin)
lit get-private-key <pkpId>      # raw derived key for a permitted PKP wallet
lit get-action-private-key       # this action's own derived key
lit get-action-public-key [cid]
lit get-action-wallet-address [cid]
lit aes-encrypt <pkpId> <msg>    # / aes-decrypt (value or "-" for stdin)
lit increment-fetch-count
```

Results go to stdout with a trailing newline; op failures (including
`ReportError` text from lit-api-server, e.g. permission denials) go to stderr
and exit non-zero. Large payloads use `-`/stdin to avoid the argv limit.

---

## Isolation model (gVisor / runsc)

The production runtime spawns **one `runsc` sandbox per execution** with an OCI
spec (`sandbox/runsc.rs`) that:

- roots at the shared read-only base rootfs with a per-exec **in-memory overlay**
  (`--overlay2=root:memory`) — the sandbox can write anywhere, nothing survives
  it, and the base image is never mutated;
- bind-mounts the bundle **read-only** at `/action`, gives writable scratch via a
  size-capped tmpfs at `/tmp`, and bind-mounts the **per-exec** op socket at
  `/run/lit/ops.sock` (`--host-uds=all` is required for the sandbox to reach it);
- runs as uid 0 *inside* the sandbox — the gVisor Sentry plus the CVM are the
  isolation boundary, so the base image needn't carry users;
- applies cgroup limits (memory / pids) via a **delegated leaf cgroup**
  (`lit-sandboxes/<exec-id>`), required to avoid cgroup-v2 `subtree_control:
  EBUSY` when nested in a container;
- uses fresh PID/IPC/UTS/mount/network namespaces.

Spike-validated invariants (2026-07-01, Phala TDX dev CVMs):

- `--host-uds=all` is mandatory (the socket exposed is per-sandbox, in a 0700
  host tempdir — never a shared one).
- Nested-in-container needs the delegated leaf cgroup; `--ignore-cgroups`
  sidesteps it for dev/tests at the cost of per-exec cgroup limits (the
  supervisor timeout still applies).
- `systrap` (default) and `ptrace` platforms both work inside TDX; there is no
  `/dev/kvm` in the CVM, so the kvm platform is not an option.

The `process` runtime (dev/CI) runs the entrypoint as a plain child process with
**no isolation** — same env vars, same op-socket location, same argv, its own
process group for whole-tree kill — so a bundle developed against it behaves
identically under runsc. It exists so the integration tests (`tests/it.rs`) can
exercise the full loop on any host, macOS included.

### Limits (first-draft defaults)

| Limit | Default | Max | Enforced by |
| --- | --- | --- | --- |
| timeout | 15 min | 150 min | supervisor |
| memory | 512 MB | 2 GB | sandbox cgroup |
| pids | 128 | — | sandbox cgroup |
| tmpfs (/tmp) | 512 MB | — | tmpfs mount size |

Memory grows on demand up to the ceiling (gVisor is not a fixed-size VM), so
per-exec `memory.peak` stays meterable for future usage-based pricing.

---

## JS runner vs gVisor runner

| Aspect | JS runner (`lit-actions`) | gVisor runner (`lit_actions_gvisor`) |
| --- | --- | --- |
| Languages | JavaScript only | Any (Python, shell, compiled binaries, …) |
| Isolation | Deno / V8 isolate | Per-execution gVisor (`runsc`) sandbox |
| `code` payload | JS source | base64 tar(.gz) bundle, or `cid:<id>` |
| Op surface | op-loop | **identical** op-loop (extern_path'd) |
| Op access from code | JS bindings | `lit` CLI / `lit_guest.proto` client |
| Socket | `/tmp/lit_actions.sock` | `/tmp/lit_actions_gvisor.sock` |
| api-server route | `/lit_action` | `/lit_binary_action` |
| Code cache | `ActionCodeCache` (api-server) | `BundleCache` (runner) |
| Termination | script returns | entrypoint process exits |
| Health gating | gates `/health` | informational only |
| Default memory | 64 MB | 512 MB |

---

## Deployment (Phala)

Ships as a new `lit-actions-gvisor` container in the existing
`docker-compose.phala.yml` — same CVM, same attested identity. It mounts the
shared `lit-socket` volume and serves its own socket there; lit-api-server
`depends_on` it and routes `/lit_binary_action` to it. The container currently
needs `privileged` to create nested runsc sandboxes (the spikes used it —
**scope down before production**).

```yaml
lit-actions-gvisor:
  image: ${DOCKER_IMAGE_LIT_ACTIONS_GVISOR}
  command: [lit_actions_gvisor, --socket, /tmp/lit_actions_gvisor.sock,
            --rootfs, /var/lib/lit/sandbox-rootfs]
  privileged: true            # nested runsc; scope down for production
  volumes: [ "lit-socket:/tmp" ]
```

Deploy caveats: the pipeline must build + substitute
`DOCKER_IMAGE_LIT_ACTIONS_GVISOR`; adding the compose service changes
`compose_hash` (**governed** in prod); bundles are bounded by the existing
`max_code_length` (large bundles may need it raised in chain config).

---

## Feature flag — off by default (CPL-359)

gVisor is opt-in on both axes:

- **Build.** The `lit_actions_gvisor` supervisor and the guest `lit` CLI are
  gated behind the `gvisor` cargo feature on `lit-actions-gvisor-server`
  (`required-features`). A default `cargo build` — and `clippy --all-targets`
  — skips them, so the runner binary is *not compiled* unless something opts
  in. `Dockerfile.lit-actions-gvisor` passes `--features gvisor` (the image
  build is the opt-in), and CI's `--all-features` keeps the binaries linted and
  tested.
- **Run.** lit-api-server always mounts `/lit_binary_action` (its OpenAPI
  surface is stable), but the `GvisorEnabled` request guard (`actions::gvisor`,
  driven by the `LIT_GVISOR_ENABLED` env var) runs *before* the CPU and billing
  guards. Unless the var is truthy (`1`/`true`/`yes`/`on`) the guard
  short-circuits with `503` — *"The gVisor any-language runner is disabled on
  this node."* — so a disabled node sheds the call without a Stripe credit check
  or dialing the runner socket. `docker-compose.phala.yml` sets
  `LIT_GVISOR_ENABLED=true` on the api-server because that stack ships the
  runner container.

The two halves are independent: an api-server image built without a runner
alongside it degrades cleanly to 503 rather than hanging on an absent socket.

---

## Security boundaries (summary)

1. **Secrets never enter the sandbox.** Key derivation and AES run *in
   lit-api-server* (in the TEE); the sandbox only issues ops and receives
   results. The op socket is per-execution, in a 0700 host tempdir.
2. **Authorization is on server-derived content ids**, never client-supplied —
   you can only execute bytes you actually submitted.
3. **Untrusted code runs in a gVisor sandbox** with a memory-overlay root
   (base image immutable), read-only bundle mount, capped tmpfs, cgroup memory/
   pid limits, and a hard supervisor timeout with `runsc kill --all` teardown.
4. **Untrusted bundles are hardened at unpack** (no symlinks/devices, stripped
   setuid, no path traversal, zip-bomb caps, manifest validated before publish).
5. **The op-loop stays strictly sequential** even under a concurrent guest
   (`OpBridge` mutex), so an adversarial multi-threaded action can't corrupt the
   protocol stream.

---

## Current status & follow-ups

As of the initial implementation ([#557](https://github.com/LIT-Protocol/chipotle/pull/557) + [#558](https://github.com/LIT-Protocol/chipotle/pull/558)):

- The crate is complete, with an end-to-end integration suite (real op-loop,
  real bundles, real `lit` CLI via the `process` runtime) plus bundle unit tests.
- runsc specifics are spike-validated but need a Linux dev CVM pass for the
  isolation/perf quality gates.

Named v1 non-goals / follow-ups:

- Warm pool / checkpoint-restore for near-instant starts (cold start ~150–500 ms,
  currently unbilled; excluding unpack/boot from billing is an api-server change).
- Egress bandwidth cap via a single proxied path (replaces the JS fetch-count
  quota; `increment-fetch-count` is proxied for parity meanwhile).
- `memory.peak` metering into `UpdateResourceUsage.used_kb` (currently 0).
- Bundle-cache eviction (currently unbounded, keyed by CID).
- Local-mode `lit` CLI deriving keys from a local private key (DX).
- OTLP telemetry export (JS runner's `otlp` feature); stdout tracing only.
- Base rootfs image build + the compose `compose_hash` governance change.
```
