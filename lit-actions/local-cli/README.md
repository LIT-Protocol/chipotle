# lit-actions-local-cli — Local `lit` CLI for any-language Lit Actions

A laptop stand-in for the **guest `lit` CLI** shipped by the any-language
runner (`lit-actions/gvisor-server`, PR #557). It lets you develop and test a
Lit Action written in **any language** — the exact same shell/Python/… code —
without a TEE, a gVisor sandbox, lit-api-server, or a network connection.

Inside the CVM, the guest `lit` CLI proxies every op over a Unix socket to
lit-api-server, which holds the root/PKP keys. This tool exposes the
**identical command surface** but resolves each op **locally**:

| Op | In the TEE (PR #557) | Locally (this tool) |
| --- | --- | --- |
| `get-private-key` / `get-action-private-key` | derived from the hardware root key | derived from a **local master key** you supply |
| `aes-encrypt` / `aes-decrypt` | AES-256-GCM with a PKP key in the TEE | AES-256-GCM with the locally-derived key |
| `print` | appended to action logs via the op-loop | appended to `.lit-local/logs` (mirrored to stderr) |
| `set-response` | recorded api-server-side | recorded in `.lit-local/response` |
| `params` / `job` / `auth-context` | supplied by lit-api-server | read from a local **job file** |
| `increment-fetch-count` | counted + quota-checked api-server-side | counted in `.lit-local/fetch_count` |

Because the surface is identical, **an action you get working here runs
unchanged in the TEE.** The *values* differ (a local key ≠ the TEE's root
key), but the encodings are byte-for-byte the same: `0x`-hex secrets,
compressed SEC1 public keys, 20-byte wallet addresses, and hex
nonce-prefixed AES-256-GCM ciphertext — all matching `lit-api-server`'s
op handlers exactly.

## Install

```sh
cargo build --release            # produces target/release/lit
export PATH="$PWD/target/release:$PATH"
lit --help
```

## Quickstart

```sh
# Pin a master key so derived keys are stable across runs (optional — one is
# generated and cached in .lit-local/ if you don't).
export LIT_LOCAL_PRIVATE_KEY=0x1111111111111111111111111111111111111111111111111111111111111111

# Describe the job your action will receive (all fields optional).
cat > lit.job.json <<'JSON'
{ "ipfsId": "QmMyAction", "jsParams": { "pkpId": "my-pkp" } }
JSON

# Run the ops directly...
lit params                       # {"pkpId":"my-pkp"}
lit get-private-key my-pkp       # 0x<64 hex>
CT=$(lit aes-encrypt my-pkp "hello")
lit aes-decrypt my-pkp "$CT"     # hello

# ...or run a whole bundle end-to-end (see below).
```

## Running a bundle: `lit run`

`lit run` is a **local-only** convenience with no counterpart in the guest
CLI (production actions never call it). It mirrors what the gvisor-server
supervisor does (CPL-355): execute **`bash startup.sh`** — the only thing a
sandbox ever runs — with `lit` on `PATH`, the job wired through the
environment, and top-level js-params injected as environment variables, then
print the recorded response.

```sh
cd examples/shell
lit run
# lit: running bash /…/examples/shell/startup.sh (cid local-action, runtime unspecified)
# hello from a shell action
# ...
# lit: --- recorded response ---
# {"ok": true, "lang": "sh"}
```

The script is the bundle's root `startup.sh`, or pass `--startup-script
other.sh` — the local analog of the `startup_script` field on
`/lit_binary_action`, which lets one bundle serve many scripts. An optional
`lit.json` (same as the sandbox's) carries metadata only:

```json
{ "runtime": "python3", "env": { "PYTHONUNBUFFERED": "1" } }
```

Serverless semantics: the run ends when the startup script exits; a non-zero
exit is propagated as this process's exit code.

See `examples/shell`, `examples/python`, and `examples/claude` — bundles
adapted from the gvisor-server examples (they exercise a few more ops for
demonstration). The bundle layout and `lit` calls are identical, so the same
bundle shape runs in both places. `examples/claude` executes the official
Claude Code installer script (`https://claude.ai/install.sh`) before running
a headless prompt (needs `curl`/`jq`, network, and an `ANTHROPIC_API_KEY` js-param); run locally it installs onto your machine.

## Command surface

```
lit job                         # full job JSON (params, authContext, headers, cid)
lit params                      # jsParams JSON (null if none)
lit auth-context                # authContext JSON (null if none)
lit print "hello"               # append to logs (mirrored to stderr)
lit set-response '{"ok":true}'  # record the response ("-" / omitted reads stdin)
lit get-private-key <pkpId>     # 0x-hex derived key for a PKP wallet
lit get-action-private-key      # this action's own derived key
lit get-action-public-key [cid] # compressed SEC1 public key
lit get-action-wallet-address [cid]
lit aes-encrypt <pkpId> <msg>   # AES-256-GCM, hex (msg "-"/omitted reads stdin)
lit aes-decrypt <pkpId> <ct>
lit increment-fetch-count       # prints the new count
lit run [--dir .] [--startup-script <file>] [--keep-state]   # local-only: run a bundle
```

## Configuration

| Flag | Env | Default | Purpose |
| --- | --- | --- | --- |
| `--key` | `LIT_LOCAL_PRIVATE_KEY` | generated + cached | 32-byte hex master key all keys derive from |
| `--state-dir` | `LIT_LOCAL_STATE_DIR` | `.lit-local` | per-run state (master key, response, logs, counter) |
| `--job` | `LIT_LOCAL_JOB` | `lit.job.json` if present | job file (params / authContext / headers / ipfsId) |
| `--ipfs-id` | `LIT_ACTION_IPFS_ID` | `local-action` | this action's content id (overrides the job file) |

`LIT_ACTION_IPFS_ID` is the same variable the gvisor-server sets in the
sandbox, so action code that reads it behaves identically here.

Add `.lit-local/` to your `.gitignore` — it holds the generated master key.

## Key derivation

Each op derives a fresh 32-byte secret per `(purpose, id)` from your master
key — `keccak256(master ‖ purpose ‖ id)`, reduced to a valid secp256k1
scalar — mirroring how the TEE derives one secret per derivation path. The
per-PKP secret doubles as the wallet key and the AES-256 key, exactly as
`get_client_key` does in lit-api-server. This is a **local development
stand-in**, not a secure key-management scheme: the master key sits in
plaintext under `.lit-local/`.

There is **no permission/scoping analog locally**: the TEE folds the caller's
API key into the derivation path (`pkp_id_to_derviation_path(api_key, pkp_id)`)
and checks that the API key may use the wallet in the action, whereas here
`get-private-key <pkpId>` derives purely from the id and always succeeds.
Values won't match production, and access control is out of scope for local
testing.

## Relationship to gvisor-server (PR #557)

This is an intentionally separate, standalone crate (its own Cargo workspace)
so it does not depend on the unmerged runner and its `lit` binary does not
collide with the guest one. It reproduces the guest CLI's surface and output
encodings; it does **not** speak the gRPC op-loop or run a sandbox. When the
any-language runner ships, the guest CLI and this local CLI stay
drop-in-interchangeable from an action author's point of view.
