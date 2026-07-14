# lit-actions-gvisor-server — Any-Language Lit Action Runner

Runs Lit Actions written in **any programming language** inside a per-execution
**gVisor (`runsc`) sandbox**, speaking the **exact same gRPC op-loop** as the JS
runner (`lit-actions/server`) — just on a different Unix socket. lit-api-server
talks to it with the identical client code; only the socket path differs.

```
lit-api-server (TDX, holds root/signer keys)
        │  gRPC op-loop over UDS (unchanged proto: lit_actions.proto)
        ▼
lit_actions_gvisor (this crate: Action service + sandbox supervisor)
        │  one runsc sandbox per execution
        ▼
runsc sandbox
   • shared read-only base rootfs + per-exec in-memory overlay (--overlay2)
   • bundle mounted read-only at /action, tmpfs at /tmp
   • fixed entrypoint: `bash /startup/startup.sh` (per-exec, read-only)
   • per-exec op socket bind-mounted at /run/lit/ops.sock (--host-uds=all)
   • preinstalled `lit` CLI exposes the ops to user code in any language
```

## Wire contract (unchanged)

The `Action.ExecuteJs` bidi stream from `lit-actions/grpc/schema/lit_actions.proto`
is reused verbatim, so all lit-api-server op handlers (`handle_ops.rs`) work
unmodified:

- `ExecutionRequest.code` carries the **bundle**: base64-encoded `tar` or
  `tar.gz` bytes, or `cid:<id>` referencing a bundle this runner already has
  cached (the any-language analog of `ActionCodeCache`).
- `ExecutionRequest.startup_script` optionally carries the **startup script**
  (see below), sent separately from the bundle so different scripts reuse the
  same cached bundle.
- `ExecutionRequest.ipfs_id` is the server-derived content id used for
  permissions/key derivation; it also keys the bundle cache.
- Ops (`Print`, `SetResponse`, `GetPrivateKey`, `AesEncrypt/Decrypt`, …) are
  proxied 1:1 from guest code onto the op-loop, strictly one in flight.
- `UpdateResourceUsage` ticks are emitted by the supervisor every 500ms
  (never by guest code); lit-api-server bills wall-clock per second and can
  cancel the action on a tick.
- Timeout ⇒ `DeadlineExceeded`, billing cancel ⇒ `ResourceExhausted`, with the
  same messages as the JS runner. Non-zero exit ⇒ failed `ExecutionResult`
  with a stderr tail.

## Bundle format & the startup script (CPL-355)

A bundle is a tarball of **payload** — binaries, scripts, data, in any
language. The sandbox only ever executes one thing: **`bash startup.sh`**,
with the bundle as the working directory. The script comes from (first wins):

1. `ExecutionRequest.startup_script` — sent with the request, mounted
   read-only at `/startup/startup.sh`. Because it travels separately from the
   bundle, one cached bundle (e.g. a large compiled binary) serves any number
   of different startup scripts with full cache hits.
2. `startup.sh` at the bundle root — for self-contained bundles.

Neither present ⇒ the execution fails before a sandbox is spawned.

The startup script runs with the `lit` CLI on PATH and every **top-level
js-param injected as an environment variable** (string values verbatim,
everything else as compact JSON). Params are skipped — never renamed — when
the name is not a valid env-var name or would shadow a runtime-owned variable
(`PATH`, `HOME`, `TMPDIR`, `LIT_OP_SOCK`, `LIT_ACTION_IPFS_ID`), or when the
value exceeds 64 KiB (1 MiB total); `lit params` always has the full data.
Typical script: read params from the environment, fetch/decrypt keys via
`lit`, then exec the bundle's binary.

An optional `lit.json` manifest at the bundle root may carry metadata:

```json
{
  "runtime": "python3",
  "env": { "PYTHONUNBUFFERED": "1" }
}
```

- `runtime` — informational for now (the v1 base image ships all supported
  runtimes).
- `env` — extra environment variables; these win over js-param injection,
  but runtime-owned names (the reserved set above) are ignored here too.
- A legacy `entrypoint` field is ignored: incoming bundles are re-written to
  the startup-script contract.

Note authorization keys on the **bundle** CID alone; the per-request startup
script is authenticated by API-key authorization, not content-addressing.

Execution semantics are serverless/CLI-style: **the run ends when the
startup script exits**. `lit set-response` only *records* the response
(api-server side); exit code 0 returns success with the last-recorded
response, non-zero returns failure. No response set ⇒ empty response.

## Guest `lit` CLI

Preinstalled in the base image (`src/bin/lit.rs`); connects to `$LIT_OP_SOCK`
(default `/run/lit/ops.sock`). Any language shells out to it, or links a gRPC
client against `schema/lit_guest.proto` for the same surface:

```sh
lit job                         # full job JSON (params, authContext, headers, cid)
lit params                      # jsParams JSON
lit print "hello"               # append to action logs (stdout/stderr are forwarded too)
lit set-response '{"ok":true}'  # record the response ("-" reads stdin)
lit get-private-key <pkpId>     # raw derived key for a permitted PKP wallet
lit get-action-private-key      # this action's own derived key
lit get-action-public-key [cid]
lit get-action-wallet-address [cid]
lit aes-encrypt <pkpId> <msg>   # / aes-decrypt
lit increment-fetch-count
```

Op failures (including errors reported by lit-api-server, e.g. permission
denials) print to stderr and exit non-zero.

## Running

```sh
# Production (inside the CVM; needs runsc + a base rootfs):
lit_actions_gvisor --socket /var/run/lit/lit_actions_gvisor.sock \
    --rootfs /var/lib/lit/sandbox-rootfs

# Local development / CI (NO isolation — never in production):
lit_actions_gvisor --runtime process --socket /tmp/lit_actions_gvisor.sock
```

Key flags/env (see `--help`): `LIT_SANDBOX_RUNTIME`, `LIT_SANDBOX_ROOTFS`,
`LIT_RUNSC_PLATFORM` (`systrap`|`ptrace` — both validated inside TDX),
`LIT_RUNSC_NETWORK`, `LIT_RUNSC_IGNORE_CGROUPS` (dev/tests; see below),
`LIT_BUNDLE_CACHE_DIR`, `LIT_GUEST_BIN_DIR` (process runtime).

### Limits (first-draft defaults; confirm in the perf gate)

| Limit        | Default | Max     | Enforced by                          |
| ------------ | ------- | ------- | ------------------------------------ |
| timeout      | 15 min  | 150 min | supervisor                           |
| memory       | 512 MB  | 2 GB    | sandbox cgroup (`linux.resources`)   |
| pids         | 128     | —       | sandbox cgroup                       |
| tmpfs (/tmp) | 512 MB  | —       | tmpfs mount size                     |

Memory grows on demand up to the ceiling (gVisor is not a fixed-size VM), so
per-exec usage stays meterable (`memory.peak`) for future usage-based pricing;
v1 billing stays flat per-second via the unchanged `UpdateResourceUsage` path.

### runsc notes (spike-validated 2026-07-01 on Phala TDX dev CVMs)

- `--host-uds=all` is required or the sandbox can't reach the op socket. The
  socket exposed is **per-sandbox** (fresh tempdir per execution), never a
  shared one.
- Nested in a container, each sandbox needs a **delegated leaf cgroup**
  (`linux.cgroupsPath = lit-sandboxes/<exec-id>`) or runsc hits cgroup-v2
  `subtree_control: EBUSY`. `--ignore-cgroups` sidesteps it for dev/tests at
  the cost of per-exec cgroup limits.
- systrap (default) and ptrace both work inside TDX; there is no `/dev/kvm`
  in the CVM so the kvm platform is not an option.

## Deployment (Phala)

Ship as a new container in the existing `docker-compose.phala.yml` (same CVM,
same attested identity — no new cross-VM trust plumbing). It mounts the same
`lit-socket` volume the JS runner uses and serves its own socket there. The
container needs enough privilege to create nested sandboxes (the spikes used
`privileged`; scope down before production). Not yet wired into compose —
adding the service changes `compose_hash`, which is a governed change in
production.

```yaml
  lit-actions-gvisor:
    image: <built from image/Dockerfile.runner>
    command: ["lit_actions_gvisor", "--socket", "/var/run/lit/lit_actions_gvisor.sock",
              "--rootfs", "/var/lib/lit/sandbox-rootfs"]
    privileged: true            # nested runsc; scope down for production
    volumes: [ "lit-socket:/var/run/lit" ]
    cpus: 2
    mem_limit: 4g
    pids_limit: 2048
```

lit-api-server connects to `/var/run/lit/lit_actions_gvisor.sock` the same way
it connects to the JS runner's socket today (`actions/client/execution.rs`).

## Local development & tests

Integration tests (`tests/it.rs`) exercise the full loop — real gRPC op-loop,
real bundles, the real `lit` CLI — using the `process` runtime, so they run
anywhere (macOS included):

```sh
cargo test -p lit-actions-gvisor-server
```

`image/` has the base-image Dockerfile; `examples/` has bundle examples.

## v1 non-goals / follow-ups

- Warm pool / checkpoint-restore for near-instant starts (mirror
  `worker_pool.rs`; cold start is ~150–500 ms per the spikes and is not billed
  — api-server currently starts its clock at stream open, so excluding
  unpack/boot time from billing is an api-server follow-up).
- Egress bandwidth cap via a single proxied path (replaces the JS fetch-count
  quota per the build plan; `increment-fetch-count` is proxied for parity in
  the meantime).
- `memory.peak` metering into `UpdateResourceUsage.used_kb` (currently 0;
  api-server ignores it and bills wall clock).
- Bundle cache eviction (unbounded, keyed by CID).
- Local-mode `lit` CLI deriving keys from a local private key (biggest DX
  unlock; the CLI surface is already designed to be drop-in interchangeable).
- OTLP telemetry export (JS runner's `otlp` feature); stdout tracing only.
