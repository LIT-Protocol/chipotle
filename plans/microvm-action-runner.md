# Any-Language Lit Action Runner — Build Plan

**Status:** Feasibility validated; ready to build
**Owner:** Chris
**Updated:** 2026-07-01

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
   • host-UDS op-loop socket mounted in → raw derived key flows in, user's lib signs, dies with tmpfs
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
   The guest/supervisor emits the same usage ticks — no new billing code. **Do not bill cold boot:**
   start the clock at *user-code start* (cold path is ~150–500 ms; hidden by the warm pool, and not
   billed regardless).
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
   Route egress through a **single proxied path** (keeps the venue-proxy feature + logging) and apply
   a **bandwidth/rate limit** instead of a call count — since users pay per second, bandwidth × time
   bounds abuse without an arbitrary cap. (User code holds the key and can exfiltrate via network —
   identical to today's JS model; the author already holds the permission.)
4. **Determinism — N/A.** Single TEE, no MPC/multi-node result comparison, so user code need not be
   deterministic.
5. **Job/response contract + termination.** Params in via the `lit` SDK/CLI; response out via a
   preinstalled CLI, e.g. `lit set-response '<json>'` (forwards over the `SetResponse` op); logs via
   `lit print`/stdout. **Termination = the entrypoint process exits** (serverless/CLI semantics) —
   `set-response` only *records* the value; on exit we return the last-recorded response and tear
   down the sandbox. Timeout/OOM are hard caps. No response set ⇒ empty response.

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

## Constraint note

All Phala/TEE experimentation stays on the **dev environment / dev.litprotocol.com** only. Do not
touch staging/prod CVMs.

## Reference
- gVisor platforms (systrap/ptrace/kvm): https://gvisor.dev/docs/architecture_guide/platforms/
