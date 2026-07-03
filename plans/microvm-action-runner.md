# Any-Language Lit Action Runner — Build Plan

**Status:** Feasibility validated; implementation spec complete — ready to build
**Owner:** Chris
**Updated:** 2026-07-03

## Goal

Let users write Lit Actions in **any programming language**, not just JavaScript. A user gets an
isolated sandbox with a full filesystem and runtime, imports whatever libraries they want (e.g. a
Solana signing lib), and the sandbox talks to `lit-api-server` over the **same gRPC op-loop** the
current JS runner uses, via a **`lit` CLI/SDK preinstalled in the sandbox image**.

## Isolation tech: gVisor (`runsc`)

We use **gVisor** to sandbox each execution. Rationale, in one breath: the user's own imported
code must do the signing, so the sandbox holds a **raw derived key** and must run **inside the TDX
CVM** (the TEE excludes the host/operator); inside the TEE, TDX gives us **no `/dev/kvm`**, so a
hypervisor (Firecracker/microVM) is off the table, and we only need **tenant-from-tenant**
isolation for arbitrary binaries — which is exactly what gVisor's userspace kernel provides with no
KVM. (For very-high-value/isolated workloads, a **fresh TDX CVM per execution** is a future
max-isolation alternative; not v1.)

**Signing is CPU-bound and gVisor runs CPU natively** (only syscalls are intercepted), so
`secp256k1`/`ed25519` math is full speed — a good fit.

## Feasibility — validated (2026-07-01)

Confirmed on real dev CVMs (scratch composes in `.context/spikes/`):
- **gVisor runs inside a Phala TDX CVM** on the **systrap** (default) and **ptrace** platforms —
  real sandboxing confirmed (guest reports `4.19.0-gvisor`, host `6.9.0-dstack`). Same on the
  **production** OS image (`dstack-0.5.9`), not just dev.
- **Two implementation facts fell out of the spikes:**
  - Launch `runsc` with **`--host-uds=all`** or the sandbox can't reach the host op-loop socket
    (default is refused). Verified both ways.
  - Give each sandbox a **delegated leaf cgroup**; otherwise `runsc` hits cgroup-v2
    `subtree_control: EBUSY` (container-in-container). `--ignore-cgroups` sidesteps it for tests.

There are no remaining "is it possible" unknowns.

## Architecture

```
lit-api-server (TDX, holds root/signer keys)
        │  gRPC op-loop (unchanged: getPrivateKey, aesDecrypt, setResponse, print, usage …)
        ▼
gVisor sandbox supervisor (new; mirrors worker_pool.rs pre-warm pattern)
        │  one runsc sandbox per execution:
        ▼
runsc sandbox = Sentry (userspace kernel) + gofer (FS proxy)
   • overlay rootfs: read-only base image  +  per-exec writable tmpfs
   • own PID / mount / network namespaces
   • per-exec host-UDS op socket mounted in → raw derived key flows in, user's lib signs, dies with tmpfs
   • preinstalled `lit` CLI/SDK exposes the ops to user code in any language
```

### Keep unchanged
- The gRPC op-loop + proto (`lit-actions/grpc/schema/lit_actions.proto`) — the language-agnostic ABI.
- All api-server op handlers: `lit-api-server/src/actions/client/handle_ops.rs`,
  `op_code_helpers/{private_keys,encryption}.rs`.
- Auth + key derivation server-side; the runner only ever gets a **derived per-PKP key**, never a root.
- Per-second billing (enforced api-server-side).

### New components to build
1. **Generalize the proto** so the op-loop is language-neutral (`ExecuteRequest` etc.); keep op
   semantics identical so the api-server op-handlers are reused verbatim.
2. **Sandbox supervisor** (replaces the Deno worker; mirror `lit-actions/server/worker_pool.rs`):
   launches `runsc` per execution with `--host-uds`, a **delegated leaf cgroup**, a per-exec
   **overlay rootfs** (read-only base + tmpfs upper), and supervisor-level timeout/memory/pids limits.
3. **Base sandbox image** with language runtimes + the preinstalled `lit` CLI/SDK.
4. **Guest `lit` CLI/SDK**: receives the job (code + params + auth_context + headers) and exposes
   the ops to user code over the host-UDS op-loop socket.
5. **Wire into `lit-api-server`** as a new runner target alongside the JS runner; ship examples in
   ≥2 non-JS languages (e.g. a Solana sign in Go + Python).

### Key existing files
- Proto: `lit-actions/grpc/schema/lit_actions.proto`
- Runner gRPC server / Unix listener: `lit-actions/server/server.rs:183`, `lit-actions/grpc/unix.rs`
- Pre-warm pool to mirror: `lit-actions/server/worker_pool.rs`
- api-server op dispatch + impls: `lit-api-server/src/actions/client/handle_ops.rs`,
  `op_code_helpers/{private_keys,encryption}.rs`
- api-server gRPC client / socket: `lit-api-server/src/actions/client/execution.rs:162`
- HTTP entry: `lit-api-server/src/core/v1/endpoints/actions.rs:24`
- Phala deploy: `docker-compose.phala.yml`, `.github/workflows/deploy-staging.yml`

## Proto & routing spec

**Today:** one service, `Action.ExecuteJs(stream ExecuteJsRequest) returns (stream ExecuteJsResponse)`
(`lit_actions.proto:5-6`). `ExecutionRequest` carries `code: string`, `js_params`, `auth_context`,
`timeout`, `memory_limit`, `http_headers`, `ipfs_id` (proto:25-33). The op union is: `set_response`,
`print`, `increment_fetch_count`, `update_resource_usage`, `aes_encrypt`, `aes_decrypt`,
`get_private_key`, `get_lit_action_{private_key,public_key,wallet_address}`, `report_error`
(proto:10-23). **lit-actions is the gRPC server; lit-api-server is the client**, connecting over the
hardcoded socket `/tmp/lit_actions.sock` (`execution.rs:190`).

**Changes:**
- Extend `ExecutionRequest` with `oneof source { string code; bytes bundle; }` (a bare CID in
  `ipfs_id` referencing a cached bundle also works, as today for JS). Entrypoint + runtime come from
  the bundle's manifest, not the proto.
- **Routing:** api-server picks the runner socket by source shape — JS `code`/`ipfs_id` → existing
  `/tmp/lit_actions.sock`; `bundle` → the sandbox runner's socket (new, e.g.
  `/tmp/lit_actions_vm.sock`, same `lit-socket` volume). Make the socket path per-runner config
  instead of the current hardcode. The JS runner is untouched; the new runner implements the same
  `Action` service, so `execution.rs` client code and all op handlers are reused verbatim.
- **HTTP entry:** extend `LitActionRequest` (`models/request.rs:169-177`) with `bundle`
  (base64, mutually exclusive with `code`/`ipfs_id`). Existing size limits already fit bundles:
  `max_code_length` 16 MB default / 160 MB cap (`client/mod.rs:24,38`).
- **v1 guest op surface:** same op set minus `increment_fetch_count` (replaced by the egress model
  below); `update_resource_usage` is emitted by the supervisor, never the guest (see billing note).

## Bundle format & manifest

- **Format:** gzipped tarball. Manifest `lit.json` at the root:
  `{ "runtime": "bin" | "python3.12" | "node22", "entrypoint": "./main.py", "env": {...} }`.
  The `bin` runtime = user ships a static binary — every compiled language works day one with zero
  runtime support from us.
- **CID:** same `IpfsHasher` CIDv0 (sha2-256 multihash) over the exact bundle bytes as JS uses
  today (`runtime.rs:288-291`, `core_features.rs:135-138`) — so `keccak256(cid)` permissions
  (`parse_with_hash.rs:74-106`) and action-key derivation (`dstack/v1/mod.rs:14-20`) reuse verbatim.
- **Extraction safety:** unpack as an unprivileged uid into a per-CID read-only dir; reject absolute
  paths, `..` traversal, symlinks/hardlinks escaping the root, device/FIFO entries; cap unpacked
  size (hard cap ~512 MB) and file count. Bundles are self-contained — **deps are vendored in the
  bundle; no network installs at runtime** (predictable cold start, artifact = what runs).
- **Cache:** per-CID unpacked layer + LRU eviction — the disk analog of `ActionCodeCache`
  (100 MB / 30-min TTL today, `runtime.rs:62-63`); ours can be bigger (~5 GB disk LRU).

## Op-loop scoping & job delivery

Today there is **no execution ID in the proto** — the per-execution bidirectional stream *is* the
auth boundary, and api-server authorizes every op against per-execution client state (api key +
ipfs_id + wallet-permission cache, `handle_ops.rs:72-127`). Preserve exactly that:

- The **supervisor** terminates each `ExecuteJs` stream, generates an `execution_id` (never trust
  guest-supplied IDs), spawns the sandbox, and creates a **per-execution guest socket**
  (`/run/lit/exec-<id>/ops.sock`, bind-mounted in — this is what `--host-uds` permits). It bridges
  guest CLI calls 1:1 onto **that execution's stream only**. A sandbox can only reach its own
  socket → cross-execution op forgery is structurally impossible, and the api-server trust model is
  unchanged.
- **Job delivery into the sandbox:** bundle unpacked read-only at `/action`; writable tmpfs workdir;
  params and auth_context as files (`/lit/params.json`, `/lit/auth_context.json`); env
  `LIT_OPS_SOCKET=/lit/ops.sock`. The guest `lit` CLI/SDK reads these — and the local-dev mock CLI
  honors the same contract.
- **stdout/stderr** are captured by the supervisor and forwarded via the `Print` op path, keeping
  the existing 100 KB console cap (`client/mod.rs:25`). `lit set-response` forwards `SetResponse`;
  the existing 1 MB response cap applies (`client/mod.rs:27`).

## Deployment topology on Phala

A dstack **"app" = one CVM = one TDX VM** running **one `docker-compose`** (the `compose_hash` is
the attested identity). Containers never split across VMs within an app; horizontal scale = N
identical full-stack VMs behind the gateway. Today it's one app / one VM with 4 containers
(`lit-api-server`, `lit-actions`, `otel-collector`, `dstack-ingress`), where `lit-actions` and
`lit-api-server` already share the `lit-socket` unix-socket volume.

**Decision — ship the runner as a new container in the existing `docker-compose.phala.yml`**
(same VM). It mounts the same `lit-socket` volume → uses the identical op-loop path; `runsc` runs
nested inside that container (which needs enough privilege to create sandboxes — the spikes used
`privileged` + `--host-uds`; scope down in production). Raw keys stay in the same attested VM, so
there's zero new cross-VM trust plumbing.

Graduate to its **own dstack app** (own VM) only when the runner needs dedicated resources or
independent scaling — the cost is a secured RA-TLS CVM↔CVM channel for the op-loop plus its own
attestation whitelisting.

**Operational note:** adding the service changes `compose_hash` → in production that's a governed
change on the `DstackApp` contract on Base (plain redeploy on dev).

## Resource model, limits, and caching

**gVisor is not a fixed-size VM** — the Sentry is a host process bounded by the cgroup the
supervisor assigns:
- **CPU** via `cpu.max` (fractional/burstable; `cpuset` to pin).
- **RAM grows on demand up to a `memory.max` ceiling** — not pre-allocated like a microVM. So we're
  **not** locked to a launch-time RAM size, and per-execution usage is meterable (`memory.peak`),
  leaving the door open to usage-based pricing later. **v1 stays flat per-second.**

**Two layers of limits:**
- **Container-level (compose):** `cpus` / `mem_limit` / `mem_reservation` / `pids_limit` on the
  runner service, so it can't starve `lit-api-server` (use non-swarm compose keys; `deploy.resources`
  is ignored outside swarm).
- **Per-execution (supervisor):** each sandbox's delegated leaf cgroup + gVisor enforce
  CPU/memory/pids/timeout, so one tenant can't starve another.

**Starting defaults** (current JS is 64 MB / 15-min; a whole runtime needs more): **1 vCPU
(burstable), memory default ~256–512 MB / max ~2 GB, tmpfs ~512 MB–1 GB, timeout 15 min default /
150 min max, plus a pids cap.** First-draft — confirm in the perf gate.

**Caching** — cache the read-only pieces, fresh writable layer per run:
- **Base image rootfs** — read-only, shared by all executions, baked into the CVM image.
- **Artifact by content hash (CID)** — unpack a bundle once into a read-only layer keyed by CID;
  same CID next call = cache hit (the any-language analog of the existing `ActionCodeCache`).
- **Optional warm pool / checkpoint-restore** for near-instant clean starts (mirror `worker_pool.rs`).
- **Clean state:** each execution gets a **fresh tmpfs upper layer + fresh namespaces** — nothing
  persists between runs though the read-only layers are cached. **Never reuse a dirty sandbox across
  tenants**; run each execution in its own fresh Sentry (or restore from a clean snapshot).

## Resolved design decisions

Grounded in how JS actions work today (file:line refs for the builder).

1. **Charging — same as JS.** Flat **$0.01/wall-clock-second** (`stripe.rs:33,774`), min 1s, via
   the `UpdateResourceUsage` op flushed every 5s (`handle_ops.rs:9,155-199`, `execution.rs:215`).
   Two mechanism notes: (a) **the supervisor emits the ticks, never the guest** — a malicious guest
   could simply not tick (today the trusted runner samples every 500 ms, `runtime.rs`); (b) billing
   elapsed is computed **api-server-side** from `execution_start`, which is set *before* boot
   (`execution.rs:215`, `handle_ops.rs:161-167`) — so **"don't bill cold boot" needs a small
   api-server change**: start the clock at the first usage tick (or an explicit started marker)
   for sandbox executions. Cold path is ~150–500 ms, hidden by the warm pool.
2. **Action identity — content hash, preserving the code-string path.** Permissions key on the
   code's IPFS CID: `keccak256(ipfs_id)` (`parse_with_hash.rs:74-106`) →
   `canUseWalletInAction(apiKeyHash, cidHash, wallet)` (`accounts/mod.rs:695-727`), and the action's
   own key derives from the CID (`dstack/v1/mod.rs:14-20`). Most users don't pin to IPFS — they send
   bytes and the server derives the CID. **The artifact is a content-addressed bundle** (tarball:
   entrypoint + manifest declaring a supported base runtime); **the user sends bundle bytes inline,
   the server hashes them to the CID** (no IPFS round-trip) for permissions + key derivation — same
   security model as JS, fast dev iteration. A CID reference to a cached bundle is also allowed.
   OCI images deferred.
3. **Egress — bandwidth cap, no fetch-count quota.** The `DEFAULT_MAX_FETCH_COUNT = 50` quota
   guarded the old MPC/many-node network against fan-out DDoS; on one TEE box that risk is gone.
   Route egress through a **single proxied path** and apply a **bandwidth/rate limit** instead of a
   call count — since users pay per second, bandwidth × time bounds abuse without an arbitrary cap.
   **Mechanism** (note: today's JS fetch is *not* an op-loop op — it's local reqwest in lit-actions,
   `op_lit_proxied_fetch` in `ext/bindings.rs:429`, 10 MB/30 s per request, venue-proxy chaining):
   the sandbox gets gVisor netstack with **no default route**; the only egress is an HTTP(S) CONNECT
   proxy the supervisor runs, exposed inside the sandbox with standard `HTTP_PROXY`/`HTTPS_PROXY`
   env set. CONNECT preserves end-to-end TLS; the proxy is the choke point for byte counting,
   throughput caps, logging, and venue-proxy chaining parity. Websockets work over CONNECT; raw
   non-HTTP TCP is unsupported in v1 (document it). Libraries that ignore proxy env have no route —
   **fail closed**. (User code holds the key and can exfiltrate via network — identical to today's
   JS model; the author already holds the permission.)
4. **Determinism — N/A.** Single TEE, no MPC/multi-node result comparison, so user code need not be
   deterministic.
5. **Job/response contract + termination.** Params in via the `lit` SDK/CLI; response out via a
   preinstalled CLI, e.g. `lit set-response '<json>'` (forwards over the `SetResponse` op); logs via
   `lit print`/stdout. **Termination = the entrypoint process exits** (serverless/CLI semantics) —
   `set-response` only *records* the value; on exit we return the last-recorded response and tear
   down the sandbox. Timeout/OOM are hard caps. No response set ⇒ empty response.

## Supervisor lifecycle & failure handling

- **Concurrency:** cap concurrent sandboxes (start at the JS pool's default of 10,
  `LIT_ACTIONS_POOL_SIZE`, `server.rs:32-33`) with a small bounded queue; overflow → error on the
  stream → HTTP 429 with retry-after. Mirror the worker-pool breaker (5 consecutive spawn failures
  → open 60 s, `worker_pool.rs:44,50`).
- **Exit mapping** — users must be able to tell these apart in the error response:
  exit 0 → last recorded `set-response` (or empty); nonzero exit → error with exit code + stderr
  tail; cgroup OOM kill (`memory.events` `oom_kill`) → "memory limit exceeded"; deadline → SIGKILL
  the `runsc` process → "timed out".
- **One sandbox per execution, never reused** — same rationale as the JS pool ("cheap to start,
  expensive to scrub", `worker_pool.rs:5-8`).
- **Crash reconciliation:** on supervisor startup, `runsc delete -f` every label-matched leftover
  sandbox and remove orphaned leaf cgroups, exec dirs, and per-exec sockets; per-execution teardown
  runs in a drop-guard so a panicking handler still cleans up.

## Base sandbox image

- **v1 runtimes:** `bin` (static binaries — Go/Rust/Zig/C covered for free), `python3.12`,
  `node22`. Add more by demand; exact versions pinned in the manifest enum.
- The image is built in CI and **pinned by digest in the compose file**, so it's part of the
  attested `compose_hash` identity — no runtime pulls.
- The `lit` CLI is a small static binary on `PATH`; language SDKs (pip/npm packages) either shell
  out to it or speak gRPC-over-UDS directly. Keep the image lean — user deps live in the bundle.

## Observability

- Per-execution structured log line (execution_id, CID, api-key hash, duration, `memory.peak`,
  egress bytes, exit class) → existing `otel-collector` container.
- Metrics: warm-pool depth, spawn latency p50/p99, active sandboxes, queue depth, OOM/timeout/error
  counts, egress bytes. `runsc` debug logs off by default; per-exec flag on dev.

## Local development & docs (build alongside, or fast-follow on adoption)

The point of "any language" is broad adoption, so **the developer/agent experience is a
first-class deliverable**, not an afterthought:

- **Skillfile + docs so an agent can build these actions.** Ship a skill (and matching docs) that
  teaches an AI agent to scaffold, write, and test a gVisor action end-to-end: the bundle/manifest
  format, the `lit` CLI surface (`set-response`, `print`, `get-private-key`, `fetch`, …), the
  params/auth_context contract, and a working example per supported language. Treat this as part of
  "done," since most authors (human or agent) will start from it.
- **Local dev via a mock `lit` CLI (future — build out as adoption grows).** Ship a local-mode `lit`
  CLI/SDK with the **same interface** as the in-sandbox gRPC CLI, but which **derives all keys
  locally from a locally-generated private key** instead of calling `lit-api-server` in a TEE. A
  developer runs their action on their laptop (in a local `runsc`/container or even bare), gets
  deterministic local keys/signatures, and iterates without deploying to Phala or spending real
  balance. When they deploy, the identical CLI surface is backed by the real op-loop. This is the
  single biggest DX unlock; we don't have to build it day one, but the interface should be designed
  so the real CLI and the mock CLI are drop-in interchangeable from the start.

## Quality gates (before shipping)

- **Isolation review** — tenant-A→tenant-B key-theft threat model; validate the two-layer (Sentry +
  CVM kernel) boundary for our config; review known `runsc` CVEs vs kernel `6.9.0-dstack`; scope
  `--host-uds` to a per-sandbox socket, not a shared one.
- **Perf / pre-warm** — warm-pool sizing, cold/warm start, a representative sign workload vs the
  current Deno path; evaluate checkpoint/restore.

## Testing & CI

gVisor's systrap/ptrace platforms run on stock Linux GitHub runners (no KVM needed), so real-runsc
integration tests run in CI, not just on dev CVMs:

- **E2E per language:** each shipped example (Go/`bin`, Python, Node) executes under real `runsc`
  against a mock op-server implementing the `Action` service; assert response, logs, key ops.
- **Malicious-bundle suite:** tar traversal / symlink escape, fork bomb (→ pids cap), memory hog
  (→ OOM exit class), egress-bypass attempts (direct dial with no proxy → fails closed), probing
  for other executions' sockets, oversized response/logs.
- **Perf:** wire a sandbox sign workload into the existing k6 perf gate on deploy-staging.

## Milestones

1. **Walking skeleton (dev-only flag):** proto `oneof source` + api-server routing + minimal
   supervisor (no pool, `bin` runtime only, `--ignore-cgroups` OK) → e2e Solana sign with a static
   Go binary on a dev CVM.
2. **Contract complete:** bundle/manifest + CID/permissions parity, Python + Node runtimes, guest
   `lit` CLI/SDK, per-exec sockets, egress proxy, exit mapping.
3. **Production shape:** delegated leaf cgroups + both limit layers, supervisor-emitted billing
   ticks + cold-boot clock change, crash reconciliation, warm pool, observability;
   malicious-bundle suite green in CI.
4. **Gate & ship (dev):** isolation review + perf gate (above), compose change on dev, docs +
   authoring skill, ≥2 non-JS examples published.

## Constraint note

All Phala/TEE experimentation stays on the **dev environment / dev.litprotocol.com** only. Do not
touch staging/prod CVMs.

## Reference
- gVisor platforms (systrap/ptrace/kvm): https://gvisor.dev/docs/architecture_guide/platforms/
