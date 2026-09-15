# lit-actions-bundler-cli — `lit-bundle`

A local developer CLI to **bundle**, **deploy**, and **run** any-language Lit
Actions against a Lit Chipotle node (CPL-352).

It is the client-side companion to the gVisor any-language runner
(`lit-actions/gvisor-server`) and lit-api-server's `/lit_binary_action`
endpoint. The bundle format and the content id (checksum) it computes match the
server **byte-for-byte**, so the CID you see locally is the one the node
authorizes, caches, and runs under.

> This is a separate binary (`lit-bundle`) from the guest/local `lit` CLI
> (`lit-actions/local-cli`). That one is the in-action op shim (`lit params`,
> `lit get-private-key`, …); this one is the outside-the-action deploy tool.

## Install

```sh
cargo build --release            # produces target/release/lit-bundle
export PATH="$PWD/target/release:$PATH"
lit-bundle --help
```

## The bundle format

A bundle is a `tar`/`tar.gz` whose root holds your files plus a `startup.sh` —
the sandbox always runs `bash startup.sh`. An optional `lit.json` carries
metadata only:

```json
{ "runtime": "python3", "env": { "PYTHONUNBUFFERED": "1" } }
```

`startup.sh` is your entrypoint: install deps, marshal secrets via the guest
`lit` CLI, do the work, and `lit set-response '…'`. Top-level `jsParams` are
injected as environment variables before it runs.

## 1. `bundle` — package a folder

```sh
# Folder already has a startup.sh:
lit-bundle bundle ./my-action                 # → writes my-action.tar.gz, prints the CID

# Folder has only a compiled binary — generate a startup.sh that runs it:
lit-bundle bundle ./my-action --binary app    # startup.sh execs ./app; app marked +x

# Supply/override the manifest, or emit a plain tar:
lit-bundle bundle ./my-action --config ./lit.json
lit-bundle bundle ./my-action --no-compress
```

The CID is printed to **stdout** (capturable); progress goes to stderr. Bundles
are deterministic (sorted entries, zeroed mtime/uid/gid), so identical inputs
produce an identical CID — a cache hit on the node instead of a re-upload.

## 2. `deploy` — register on the network

```sh
export LIT_API_URL=https://your-node.example
export LIT_API_KEY=sk-...

lit-bundle deploy ./my-action --name "my-action" --description "does a thing"
```

Registers the bundle's CID on the node via `POST /core/v1/add_action` so your
API key is permitted to run it, and prints the CID. (`deploy` does not send the
bundle bytes — those ride along on the first `run`, and the node caches them
under the CID thereafter.)

## 3. `run` — execute; params become jsParams

```sh
# First run from a folder/bundle sends the bytes:
lit-bundle run ./my-action --param pkpId=my-pkp --param mode=sign

# Repeat runs reference the cached bundle by CID — no bytes re-sent:
lit-bundle run --checksum Qm... --param pkpId=my-pkp

# Full JSON params, or override this run's entrypoint:
lit-bundle run ./my-action --params-json '{"a":1,"b":[2,3]}'
lit-bundle run --checksum Qm... --startup-script ./alt-startup.sh
```

`--param key=value` sets string params; `--params-json` sets a full object
(`--param` overrides individual keys). The action's `response` prints to stdout;
its logs print to stderr.

## Configuration

| Flag | Env | Default |
|---|---|---|
| `--api-url` | `LIT_API_URL` | `http://localhost:8080` |
| `--api-key` | `LIT_API_KEY` | (required for deploy/run) |

## Development

```sh
cargo test        # drives the real binary; covers bundling + CID determinism
cargo clippy --all-targets
cargo fmt
```

Standalone workspace on purpose (own `[workspace]` table): a local dev tool, kept
out of the lit-actions workspace build and named `lit-bundle` so it never
collides with the guest `lit` binary in a shared target dir.
