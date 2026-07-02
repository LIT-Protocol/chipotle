# Any-Language Lit Action Runner — Plan

**Status:** Draft / spike phase
**Owner:** Chris
**Created:** 2026-07-01
**Updated:** 2026-07-01

## Goal

Let users write Lit Actions in **any programming language**, not just JavaScript. A user
gets an isolated sandbox with a full filesystem and runtime, imports whatever libraries they
want (e.g. a Solana signing lib), and the sandbox talks to `lit-api-server` over the **same
gRPC op-loop** the current JS runner uses, via a **CLI/SDK preinstalled in the sandbox image**.

Original idea: [bhatti](https://github.com/sahil-shubham/bhatti) (Go orchestrator around
Firecracker + jailer) microVMs, ideally inside a Phala TEE. **That specific combo is not
viable** — see below — but the goal stands, and there's a clean path to it.

---

## Two hard constraints that determine the whole design

### Constraint 1 — the runner must hold raw derived key material (preserve the paradigm)

The entire value of Lit Actions is **bring-your-own-crypto**: the user imports any signing
library and signs whatever they want (Solana, Cosmos, some chain we've never heard of) without
us implementing it server-side. We refuse to get stuck maintaining per-user signing algorithms.

Consequence: we **cannot** use a "scoped op" model (where the runner asks the api-server to
`sign(hash)` and never sees the key). The user's own imported code does the signing, so the
runner **must receive the raw derived private key**.

Confirmed against the proto (`lit-actions/grpc/schema/lit_actions.proto`): the ops that cross
into the runner today already return raw material —
`GetPrivateKeyResponse.secret` (raw per-PKP key), `GetLitActionPrivateKeyResponse.secret`,
`AesDecryptResponse.plaintext`. There is **no `sign` op**; the runner calls `getPrivateKey` and
signs itself. This is why `lit-actions` lives in the TEE today.

### Constraint 2 — if the runner holds raw keys, it must be inside the TEE

If the runner holds a raw derived key, then anyone who can read runner memory can exfiltrate a
**persistent** user key (sign as that user until rotation). To stop the operator/host from doing
that, the runner must run **inside the TDX CVM**.

**Therefore: Firecracker-outside-the-TEE is dead for the general case** (it would expose derived
keys to the operator). And Firecracker-*inside*-the-TEE is impossible because **Intel TDX guests
have no nested `/dev/kvm`** (traditional nested virt is forbidden by TDX design; TDX 1.5 "TD
Partitioning" is not stock KVM-in-guest and Phala doesn't expose it). So **Firecracker is out
either way.**

---

## Reframing: inside the TEE, the problem gets easier

Once the runner is inside the TDX CVM, the threat model shrinks to a well-trodden one:

- The **host/operator is already excluded by the hardware** — they can't see into the CVM.
- The **guest kernel is attested** and part of the measured image — trusted.
- The **only** remaining threat is **tenant A escaping its sandbox to read tenant B's derived
  key** from another execution's memory.

So we are *not* defending against a malicious host (the TEE did that). We need a strong
**tenant-from-tenant** boundary, with **no `/dev/kvm`**, that can **run any binary**. Narrow,
solved problem.

---

## Option space: no-KVM, any-language isolation inside a TEE

| Option | Tenant isolation | Any binary? | No KVM? | Notes |
|---|---|---|---|---|
| **gVisor (`runsc`)** | strong (2nd userspace kernel) | **yes** | **yes** (ptrace/systrap) | Purpose-built for untrusted multi-tenant code; what GKE Sandbox / Cloud Run use. **Primary choice.** |
| Hardened container (runc + seccomp/Landlock/userns) | weak-ish | yes | yes | One shared kernel; a kernel LPE = cross-tenant key read. gVisor exists to fix exactly this. |
| QEMU / Cloud-Hypervisor **TCG** (software emulation) | VM-grade | yes | yes | A real VM with no KVM — but pure emulation is ~10–50× slower CPU. Too slow for sign-heavy actions. |
| WASM (wasmtime) | strong | **no** | yes | Fails "import the Solana lib and sign." Out. |
| **Fresh TDX CVM per execution** | total (separate VM) | yes | n/a | Isolation = the TEE boundary itself, no sandbox needed. But per-exec CVM boot is seconds + Phala provisioning cost. The **max-isolation alternative**. |

Two real survivors: **gVisor** (fast, shared-CVM, sandbox boundary) and **per-execution CVM**
(slow, total isolation). gVisor is the pragmatic winner; per-exec-CVM is the extreme fallback.

Firecracker is demoted: only viable *outside* the TEE, which breaks key custody (Constraint 2).

---

## Primary architecture: gVisor sandboxes inside the Phala TDX CVM

```
lit-api-server (TDX, holds root/signer keys)
        │  gRPC op-loop (unchanged: getPrivateKey, aesDecrypt, setResponse, print, fetch-count …)
        ▼
gVisor sandbox supervisor (new, inside same CVM)  ── mirrors worker_pool.rs pre-warm pattern
        │  each execution:
        ▼
runsc sandbox = Sentry (userspace kernel) + gofer (FS proxy)
   • own overlay rootfs: read-only base image  +  per-exec writable tmpfs
   • own PID / mount / network namespaces
   • op-loop socket mounted in  → raw derived key flows in, user's imported lib signs, dies with tmpfs
   • preinstalled `lit` CLI/SDK exposes the ops to user code in any language
```

### How the per-execution filesystem works
1. **Base image** (the "sandbox image" analog): read-only rootfs with language runtimes + the
   preinstalled `lit` CLI/SDK + optional cached libs. Built once, measured as part of the TEE image.
2. **Per execution:** a new `runsc` sandbox with an **overlay rootfs** — read-only base as the
   lower layer + a **writable `tmpfs` upper layer** unique to this run. Clean, private, ephemeral
   FS; all writes vanish when the sandbox dies. (This is the "each execution gets its own virtual
   filesystem" property.)
3. Own PID/mount/network namespaces — user code sees a whole isolated Linux box.
4. Syscalls are intercepted by the **Sentry** in userspace; user code never touches the real CVM
   kernel directly. To escape: need a Sentry bug **and then** a CVM-kernel bug (two layers).
5. The **op-loop channel** to `lit-api-server` is handed in as a mounted socket. Raw keys flow in,
   are used, and never leave the TEE.

### Performance note (favorable)
Signing is CPU-bound, and gVisor runs CPU **natively** — only *syscalls* are intercepted. So
`secp256k1`/`ed25519` math is full speed; overhead lands only on I/O syscalls and `fetch`. Good
fit for sign-oriented actions.

### Pre-warming
Maps onto the existing `worker_pool.rs` pattern: keep N warm `runsc` sandboxes, or use gVisor
checkpoint/restore for fast resume (same warm/cold idea bhatti uses, minus the hypervisor).

---

## What we keep vs. what's new

**Keep unchanged:**
- The gRPC op-loop + proto contract (`lit-actions/grpc/schema/lit_actions.proto`). The ops
  (`getPrivateKey`, `aesDecrypt`, `setResponse`, `print`, fetch-count/billing) are the
  language-agnostic ABI. No scoped-op redesign — raw material still crosses the boundary.
- All api-server op handlers: `lit-api-server/src/actions/client/handle_ops.rs`,
  `op_code_helpers/{private_keys,encryption}.rs`.
- Auth + key derivation server-side; runner still only ever gets a **derived per-PKP key**, never
  a root — same as today.
- Wall-second billing + fetch counting (enforced api-server-side, carry over).

**New:**
- A **gVisor sandbox supervisor** replacing the V8/Deno worker (`lit-actions/server/worker_pool.rs`
  is the pattern to mirror; timeout/memory enforced at supervisor level, not V8 heap callbacks).
- A **base sandbox image** with language runtimes + preinstalled `lit` CLI/SDK.
- A **guest `lit` CLI/SDK** that receives the job (code + `js_params` + `auth_context` + headers)
  and exposes the ops to user code over the op-loop socket.
- Transport inside the CVM stays a Unix/vsock socket (same as `/tmp/lit_actions.sock` today).

### Key existing files
- Proto / contract: `lit-actions/grpc/schema/lit_actions.proto`
- Runner gRPC server / Unix listener: `lit-actions/server/server.rs:183`, `lit-actions/grpc/unix.rs`
- Pre-warm pool pattern to mirror: `lit-actions/server/worker_pool.rs`
- api-server op dispatch + impls: `lit-api-server/src/actions/client/handle_ops.rs`,
  `.../op_code_helpers/{private_keys,encryption}.rs`
- api-server gRPC client / socket: `lit-api-server/src/actions/client/execution.rs:162`
- HTTP entry: `lit-api-server/src/core/v1/endpoints/actions.rs:24`
- Phala deploy: `docker-compose.phala.yml`, `.github/workflows/deploy-staging.yml`
- Attestation: `lit-api-server/src/dstack/v1/dstack.rs`, `endpoints.rs`

---

## Spike 1 result — ✅ DONE (2026-07-01): gVisor runs inside a Phala TDX CVM

Deployed a throwaway `gvisor-spike` CVM (tdx.small, image `dstack-dev-0.5.9`, kernel
`6.9.0-dstack`), downloaded `runsc release-20260622.0`, and booted sandboxes. **Result:**

- **systrap platform (modern default): WORKS.** **ptrace platform: WORKS.** Confirmed real
  sandboxing, not passthrough — inside the sandbox `uname` reports `4.19.0-gvisor` (vs host
  `6.9.0-dstack`) and dmesg shows gVisor's `Starting gVisor... / Synthesizing system calls...`
  banner.
- **kvm platform: N/A** — `/dev/kvm` absent (confirms TDX has no nested KVM, as predicted).
- Kernel prerequisites all present: `tdx_guest` cpuinfo flag (real TDX), **user namespaces
  enabled** (`user.max_user_namespaces = 7360`), **no yama `ptrace_scope`** restriction, no
  seccomp on the host process.
- **One real gotcha, now understood:** the first attempt failed for *all* platforms at the same
  pre-platform step — `cgroup.subtree_control: device or resource busy`. This is the classic
  cgroup-v2 "no internal processes" container-in-container issue (runsc tried to enable
  controllers on a cgroup that already held PID 1), **not** a gVisor/kernel blocker. Fixed
  instantly with `runsc --ignore-cgroups`. **Production implication:** the sandbox supervisor
  must give each `runsc` sandbox a properly *delegated* leaf cgroup (or run with cgroup handling
  disabled and enforce limits ourselves) — a solved problem, but a design item for the supervisor.
- **Access pattern learned:** `phala deploy` auto-attaches `~/.ssh/id_*.pub` and picks a dev OS
  image, so `phala ssh --cvm-id … -- 'docker exec <container> …'` gives a fast interactive loop —
  no redeploy needed to iterate inside a running CVM.

**Scratch compose:** `.context/spikes/docker-compose.gvisor-spike.yml`.

---

## Spike 2 result — ✅ DONE (2026-07-01): prod-image parity + host-UDS socket reachability

Original "spike 2" was going to build a dummy op-loop server and prove a non-JS gRPC round-trip.
**Dropped as redundant** — gRPC over a unix socket is the production path today, `lit-api-server`
*is* the op-loop server, and protobuf has codegen for every language. The round-trip isn't in
doubt. What *was* genuinely unverified were two cheap things, now both closed on a `gvisor-uds`
CVM deployed on the **production** OS image (`dstack-0.5.9`, non-dev):

- **Production-image parity: ✅.** gVisor boots identically on the prod image — sandbox reports
  `4.19.0-gvisor`, host `6.9.0-dstack`, `tdx_guest` present. No dev-vs-prod kernel difference that
  matters for us.
- **Host-UDS gate (the one gVisor-specific wrinkle): ✅ understood.** By default a gVisor sandbox
  **cannot** connect to a host unix domain socket — proven: without the flag, `socat` in the
  sandbox got `connect(/tmp/op.sock): Connection refused`. Launch `runsc` with **`--host-uds=all`**
  and it works — full round trip returned `PONG_FROM_HOST`. So the guest reaches the
  `lit-api-server` socket iff the supervisor launches `runsc --host-uds=…` (or we expose the op-loop
  over vsock/TCP instead). Known flag, not a research question.

Scratch compose: `.context/spikes/docker-compose.gvisor-uds.yml`.

**Both feasibility gates are now green.** No remaining "is it possible" unknowns — the rest is
implementation. Remaining *research* items (isolation review, perf) are quality gates, not
blockers, and move below.

---

## Feasibility: settled. What's left is implementation + quality gates.

### Implementation phases (this is the actual work — not spikes)
Deploy the **real `lit-api-server`** to a dev Phala box and build the runner against it:

1. **Generalize the runner protocol.** Fork/rename the proto so the op-loop is language-neutral
   (`ExecuteRequest` etc.); keep op semantics identical so api-server op-handlers are reused verbatim.
2. **Sandbox supervisor** (replaces the Deno worker; mirror `worker_pool.rs`): launches `runsc`
   per execution with `--host-uds`, a **delegated leaf cgroup** per sandbox (spike-1 fix), a
   per-exec **overlay rootfs** (read-only base + tmpfs upper), and supervisor-level timeout/memory
   enforcement.
3. **Base sandbox image** with language runtimes + the preinstalled `lit` CLI/SDK.
4. **Guest `lit` CLI/SDK**: receives the job (code + params + auth_context + headers), exposes the
   ops to user code over the host-UDS op-loop socket.
5. **Wire into `lit-api-server`** as a new runner target alongside the JS runner; examples in ≥2
   non-JS languages (e.g. a Solana sign in Rust/Go/Python).

### Quality gates (before shipping — not feasibility blockers)
| Gate | Question |
|---|---|
| **Isolation review** | Threat-model tenant-A→tenant-B key theft; validate the two-layer (Sentry + CVM kernel) argument for our config; review known `runsc` CVEs vs kernel `6.9.0-dstack`; decide `--host-uds` scope (per-sandbox socket, not shared). |
| **Perf / pre-warm** | Warm-pool of `runsc` sandboxes (mirror `worker_pool.rs`); measure cold/warm start + a representative sign workload vs the current Deno path; evaluate gVisor checkpoint/restore. |
| **Alt path (optional)** | Per-execution TDX CVM for max-isolation/high-value workloads — measure Phala provisioning/boot latency + cost. |

---

## Recommendation

Build **gVisor sandboxes inside the Phala TDX CVM**, one per execution, each with an overlay
rootfs (read-only base + per-exec tmpfs), all speaking the existing op-loop. It preserves the
bring-your-own-crypto paradigm (raw key in, user's own lib signs), keeps keys in the TEE, needs
no `/dev/kvm`, and runs any language.

- **Firecracker was never the point** — *VM-grade tenant isolation without a hypervisor* was, and
  gVisor is the right tool inside a TEE.
- **Per-execution TDX CVM** is the max-isolation alternative if gVisor's boundary proves
  insufficient (isolation review) or for special high-value workloads — at the cost of
  seconds-scale boot.

**Feasibility is settled and green** — gVisor runs on systrap + ptrace inside a real TDX CVM
(prod image), and a sandboxed process reaches the host op-loop socket via `--host-uds`. There are
no remaining "is it possible" unknowns. **Next is implementation**: deploy the real
`lit-api-server` to a dev Phala box and build the runner against it (phases above).

---

## Deployment topology on Phala

### How Phala/dstack works
A dstack **"app" = one CVM = one TDX VM** running **one `docker-compose`**. The compose file's
`compose_hash` is what gets measured and attested — it *is* the app's identity. Multiple
*containers* run inside that single VM; services are **never** split across VMs within an app.
An app *can* be replicated to **N identical VMs** behind Phala's gateway for horizontal scale, but
each VM runs the *whole* compose — so it's "1 app → N identical full-stack VMs," never "services
sharded across VMs." To give a component its own VM/resources, it must be its **own app**.

### Today
One app, one VM (replicable), 4 containers in `docker-compose.phala.yml`:
`lit-api-server` (Rocket :8000, holds keys, mounts `/var/run/dstack.sock`), `lit-actions`
(JS runner, gRPC over the shared `lit-socket` volume), `otel-collector`, `dstack-ingress`
(RA-TLS :443 → api-server). Note `lit-actions` and `lit-api-server` are already separate
images/containers sharing the `lit-socket` unix-socket volume — that's the seam the new runner
slots into.

### Decision: Option A — new container in the existing compose (same VM)
Start the gVisor runner as **another service in `docker-compose.phala.yml`**, in the same CVM:

- New service (e.g. `lit-runner-gvisor`) mounts the same `lit-socket` volume → talks to
  `lit-api-server` over the **identical op-loop path** the JS runner uses. Roughly "add a service
  block."
- `runsc` runs *inside* that container (nesting: CVM → container → gVisor sandbox). The container
  needs the privileges exercised in the spikes (`privileged` / `SYS_ADMIN`-ish + the `--host-uds`
  path); production should scope these down to the minimum that lets `runsc` create sandboxes.
- **Shares the VM's attestation boundary and key-custody context** — raw keys stay inside the same
  attested VM, so there is **zero new cross-VM trust plumbing**. Simplest and safest.

**Graduate to Option B (its own dstack app / own VM) later** only when the runner needs dedicated
resources, independent scaling/upgrade cadence, or a distinct trust profile. Cost of graduating: a
secured CVM↔CVM channel (RA-TLS/mTLS between two attested apps) for the op-loop, plus its own
attestation whitelisting. Raw-key custody still requires it to be a TEE, so a separate CVM is fine
trust-wise — the cross-app channel is the new thing to secure.

### Two operational flags
1. **`compose_hash` changes are governed in production.** Adding the service changes the hash; in
   prod the new hash + image must be whitelisted on the `DstackApp` contract on Base (governed
   change, not a plain redeploy). On dev it's just a redeploy.
2. **Resource contention** — see below.

### Resource limits for the new container
Today the compose sets **no** per-service `cpus`/`mem_limit`, so a heavy action could starve
`lit-api-server` (noisy neighbor) since they share one VM. dstack runs plain `docker compose`
(not swarm), so use the **non-swarm** compose keys (the `deploy.resources` block is ignored
outside swarm). Apply **two layers**:

- **Container-level cap (compose)** — bounds the runner's *total* footprint so `lit-api-server`
  always has headroom. On the `lit-runner-gvisor` service set:
  - `cpus:` — e.g. leave ≥1 vCPU reserved for api-server + ingress + otel; cap the runner at the
    remainder.
  - `mem_limit:` + `mem_reservation:` — hard ceiling + soft floor.
  - `pids_limit:` — cap total processes (defense against fork bombs across sandboxes).
- **Per-execution cap (supervisor)** — the runner supervisor gives *each* `runsc` sandbox its own
  **delegated leaf cgroup** (the spike-1 fix) with per-execution CPU/memory/pids limits + timeout,
  mirroring today's per-action 64 MB / 15-min budget. gVisor enforces the sandbox side; the leaf
  cgroup enforces the host side.

Net: compose limits keep the *runner as a whole* from starving its VM-mates; the supervisor's
per-sandbox cgroups keep one tenant's action from starving another. Revisit the compose split (→
Option B) once perf data shows the shared VM is the bottleneck.

---

## Resolved design decisions (handoff)

All decided with the team (2026-07-01). Grounded in how JS actions work today (file:line refs).

### 1. Charging / payment — reuse JS pricing; no cold-start charge
JS actions bill **flat $0.01 per wall-clock second** (`COST_LIT_ACTION_PER_SECOND_CENTS = 1`,
`stripe.rs:33`; charge `stripe.rs:774`), **min 1s**, accumulated via `UpdateResourceUsage` and
flushed every 5s (`handle_ops.rs:9,155-199`, `execution.rs:25-49,215`).
- **✅ Reuse the exact same pricing + metering primitive.** Guest SDK / supervisor emits
  `UpdateResourceUsage` ticks over the op-loop; the per-second Stripe path is unchanged. No new
  billing code.
- **✅ Do not charge for cold boot.** Start the billing clock at *user-code start*, not sandbox
  provisioning. `execution_start` is set api-server-side before the stream (`execution.rs:215`) —
  the runner must signal "user code started" so the clock reflects user time, not our cold start.
- **Cold-boot estimate:** plain `runsc` sandbox start is ~**100–300 ms** (Sentry + gofer), plus the
  in-sandbox runtime init (static Go/Rust binary ~single-digit ms; Python ~30–100 ms; Node
  ~100 ms). So realistic cold path ~**150–500 ms**. **Mitigated to ~ms via a warm pool** (pre-booted
  sandboxes, one-execution-each, mirroring `worker_pool.rs`) and/or gVisor checkpoint/restore
  (tens of ms). Net: users shouldn't perceive cold start once pre-warming exists, and either way
  they aren't billed for it.
- Resource-tier pricing stays a **future hook**, not v1 (see resource model below).

### 2 + 3. Action identity, artifact format, and the "code string" fast path — combined decision
Authorization is keyed on the **content hash (IPFS CID) of the action code**: `ipfs_cid_to_u256 =
keccak256(ipfs_id)` (`parse_with_hash.rs:74-106`) → on-chain `canUseWalletInAction(apiKeyHash,
cidHash, walletAddress)` (`accounts/mod.rs:695-727`, cached `op_code_helpers/mod.rs:34-50`), and the
action's own key derives from the CID (`get_lit_action_key("lit_action_{ipfs_id}")`,
`dstack/v1/mod.rs:14-20`). **Key insight from the team:** most users don't actually pin to IPFS —
they send a **code string**, and `lit-api-server` **derives the CID from the bytes server-side**.
IPFS pinning is slow and painful during dev iteration, so we preserve the "send bytes, we hash"
model.
- **✅ Artifact = a content-addressed bundle** loaded into the preinstalled base image. The bundle
  (tarball: entrypoint + a manifest declaring the base runtime from a supported set + any files) is
  the "code".
- **✅ Preserve the code-string fast path.** The user **sends the bundle bytes inline**; the server
  **derives the CID by hashing the bytes** (no IPFS round-trip, no pinning) and uses that CID for
  permissions + key derivation — identical security model to JS today. Optionally a user *may* pass
  a CID reference to a previously-cached/pinned bundle (production reuse), mirroring today's "inline
  code OR `ipfs_id`" choice. Dev iteration stays fast; identity stays a content hash.
- **Defer OCI images** to a later phase (pull latency, registry trust, larger measured surface).

### 4. Network egress — drop the fetch *count* quota; use a bandwidth cap instead
The fetch-count quota (`DEFAULT_MAX_FETCH_COUNT = 50`, `handle_ops.rs:57-70`) exists for a threat
that **no longer applies**: it guarded the old **MPC / many-node** network against a single action
fanning out a DDoS across all nodes. Everything now runs on **one TEE box**, so that risk is gone.
- **✅ Replace the per-call count quota with a bandwidth/rate limit.** Since users already pay
  **per second of execution**, a bandwidth ceiling (bytes/sec) naturally bounds abuse: limited
  bandwidth × billed seconds ≈ the same protection, without an arbitrary "50 fetches" cap that
  confuses users. Implement as an egress rate limit on the sandbox's network path (tc/netem on the
  proxied path, or gVisor netstack throttling). Keep the existing 10 MiB per-response cap
  (`PROXIED_FETCH_MAX_BYTES`) if useful; drop the count.
- **Egress path:** keep a **single proxied egress path** (for the venue-proxy feature + egress
  logging + where the bandwidth cap is applied) rather than raw sandbox networking. No hard domain
  allowlist (parity with today).
- **Note (not a new risk):** user code holds the derived key and can make network calls, so it can
  exfiltrate the key — **identical to today's JS model**; the action author already holds the
  permission.

### 5. Resource limits & timeouts — starting defaults (from current JS limits)
Current JS limits (chain-config-driven via `ConfigKeys::LIT_ACTION_DEFAULT_*`, with `MAX_MAX_*`
ceilings): **memory default 64 MB**; **timeout default 15 min, max 150 min** (`MAX_TIMEOUT_MS =
DEFAULT_TIMEOUT_MS * 10`, `client/mod.rs:19-34`); **fetch count default 50** (being replaced, see
#4); per-fetch 10 MiB / 30 s caps; **pool size 10**.
- **✅ Pick starting numbers now, tune later.** Proposed runner v1 defaults (a whole OS/runtime
  needs more than a 64 MB JS heap): **1 vCPU** (burstable), **memory default ~256–512 MB / max
  ~2 GB**, **tmpfs disk ~512 MB–1 GB**, **timeout: reuse 15 min default / 150 min max**, plus a
  **pids limit** (fork-bomb guard). Enforced by the supervisor's per-sandbox delegated cgroup.
  Treat these as first-draft; confirm against real workloads in the perf gate.

### 6. Determinism / multi-node consensus — ✅ N/A
Confirmed with the team: everything runs in a **single TEE**, no MPC / multiple nodes / result
comparison. **User code need not be deterministic.** No consensus constraint on the runtime.

### 7. Guest job/response contract + termination — ✅ decided
- **Params in:** the job (`js_params` + `auth_context` + `http_headers` + the bundle) is delivered
  to the guest; the `lit` SDK/CLI (preinstalled in the base image) exposes them to user code.
- **Response out:** the user calls a **preinstalled CLI**, e.g. `lit set-response '<json>'`, which
  forwards the value over the op-loop `SetResponse` op. Logs via `lit print` / stdout.
- **✅ Termination = the user's entrypoint process exits** (natural serverless/CLI semantics). No
  explicit "I'm done" call: `set-response` just *records* the value; when the entrypoint (sandbox
  PID 1) exits, gVisor tears down the whole sandbox (children included) and we return the
  last-recorded response. Timeout and OOM are hard caps that also terminate. If they never call
  `set-response`, the response is empty/null.

---

## Resource allocation model (gVisor specifics)

A gVisor sandbox is **not a fixed-size VM** — the Sentry is a host process constrained by the
**cgroup** the supervisor assigns it. This is an advantage over a microVM for our billing:

- **CPU:** set via the sandbox's cgroup (`cpu.max` quota — e.g. ~1 core, fractional or burstable;
  `cpuset` to pin). Meterable as CPU-seconds if we ever want it.
- **RAM grows on demand up to a ceiling.** gVisor allocates guest memory lazily as the workload
  uses it — it is **not pre-allocated** the way Firecracker fixes RAM at boot. The supervisor sets a
  cgroup **`memory.max` ceiling** (the tier max) for safety; actual usage floats up to it. So the
  answer to "are we stuck with the RAM level at launch?" is **no** — usage is dynamic within the
  ceiling.
- **Charging for growth is therefore possible** (a gVisor-specific win): `memory.peak` / cgroup
  accounting gives us per-execution memory usage, so a future tier/usage-based price can bill on
  observed memory-seconds. **v1 stays flat per-second** (decision #1); we just make sure the
  accounting is available so we're not blocked from usage pricing later.

## Caching model

Cache the expensive read-only pieces; give every execution a clean, ephemeral writable layer.

- **Layer 1 — base image rootfs:** read-only lower layer with runtimes + `lit` CLI, shared by
  **all** executions, always warm (baked into the CVM image).
- **Layer 2 — artifact cache by content hash (CID):** unpack a bundle once into a read-only layer
  keyed by its CID; a second call with the same CID is a cache hit — no re-upload/re-unpack. This is
  the any-language analog of the existing `ActionCodeCache` / `ipfs_id` code cache.
- **Layer 3 — warm sandbox pool** (optional): pre-booted `runsc` sandboxes ready to accept work
  (mirror `worker_pool.rs`), and/or **checkpoint/restore** from a clean "runtime-loaded, pre-entry"
  snapshot for near-instant clean starts.
- **Clean state between runs (the key requirement):** each execution gets a **fresh `tmpfs` upper
  layer** in the overlay + fresh PID/mount/net namespaces, so **nothing persists between runs** even
  though Layers 1–2 are cached. Exactly the "cache the image, clean tmpfs each run" model.
- **Security rule:** never reuse a *dirty* sandbox across executions/tenants — reuse the cached
  read-only layers, but run each execution in its **own fresh sandbox instance** (fresh Sentry) or a
  **restore-from-clean-snapshot**, so there is no memory or filesystem residue between tenants. (The
  current JS pool already follows this: one execution per worker, then dropped.)

---

## Constraint note

Per standing guidance: all Phala/TEE experimentation stays on the **dev environment /
dev.litprotocol.com** only. Do not touch staging/prod CVMs for spikes.

## Sources
- Intel TDX — Linux Kernel docs (nested virt / TD Partitioning): https://docs.kernel.org/virt/kvm/x86/intel-tdx.html
- gVisor architecture / platforms (ptrace, systrap, KVM): https://gvisor.dev/docs/architecture_guide/platforms/
- Firecracker issue #1721 (KVM inside guest): https://github.com/firecracker-microvm/firecracker/issues/1721
- Firecracker without KVM (why it can't, gVisor as the answer): https://blog.alexellis.io/how-to-run-firecracker-without-kvm-on-regular-cloud-vms/
- Intel TDX for KVM mainlined in Linux 6.16 (Phoronix): https://www.phoronix.com/news/Intel-TDX-For-KVM-Linux-6.16
- bhatti: https://github.com/sahil-shubham/bhatti
