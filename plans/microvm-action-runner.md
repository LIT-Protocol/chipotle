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

## Spikes (priority order)

| # | Spike | Answers | Cost |
|---|-------|---------|------|
| **1** | **Boot a `runsc` hello-world inside a dev Phala CVM.** gVisor's ptrace/systrap platform needs `PTRACE`/`seccomp` and possibly specific caps a locked-down guest might restrict. Confirm `runsc` runs at all. | Is the primary architecture viable inside TDX? Replaces the old "/dev/kvm?" spike. | small |
| **2** | **Per-exec overlay rootfs + op-loop socket:** run a non-JS guest (Go/Python) in `runsc` that does one `getPrivateKey` + local sign + `setResponse` round-trip against a local api-server, over a mounted socket, with a per-exec tmpfs overlay. | Proves the language-agnostic ABI end-to-end + the FS isolation model. | medium |
| **3** | **Sandbox escape / isolation review of gVisor inside TDX.** Threat-model tenant-A→tenant-B key theft; confirm the two-layer (Sentry + CVM kernel) argument holds for our config; check known `runsc` CVEs vs our kernel. | Is "tenant isolation inside the TEE" actually sufficient for raw-key custody? | medium |
| **4** | **Pre-warm + perf:** warm-pool of `runsc` sandboxes (mirror `worker_pool.rs`); measure cold/warm start + a representative sign workload vs the current Deno path. | Latency/throughput acceptable? Checkpoint/restore needed? | medium |
| **5** | **(Alt path) Per-execution TDX CVM:** measure Phala CVM provisioning/boot latency + cost for the max-isolation model. | Is per-exec-CVM ever worth it (high-value/long-running actions)? | small (external) |

**Spikes 1 and 2 gate everything.** Spike 1 says whether `runsc` runs in the CVM at all; spike 2
proves the any-language op-loop + per-exec FS. Image build, supervisor, guest SDKs, and examples
all wait on those.

---

## Recommendation

Build **gVisor sandboxes inside the Phala TDX CVM**, one per execution, each with an overlay
rootfs (read-only base + per-exec tmpfs), all speaking the existing op-loop. It preserves the
bring-your-own-crypto paradigm (raw key in, user's own lib signs), keeps keys in the TEE, needs
no `/dev/kvm`, and runs any language.

- **Firecracker was never the point** — *VM-grade tenant isolation without a hypervisor* was, and
  gVisor is the right tool inside a TEE.
- **Per-execution TDX CVM** is the max-isolation alternative if gVisor's boundary proves
  insufficient (spike 3) or for special high-value workloads — at the cost of seconds-scale boot.

Do **spike 1 today**: it's the one thing that can kill the primary architecture, and it's a
`runsc` hello-world in a dev CVM.

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
