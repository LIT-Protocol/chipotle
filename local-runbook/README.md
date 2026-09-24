# Chipotle — Local Install & Run Runbook (offline runtime)

Stand up the full Chipotle stack on a **single macOS or Linux machine** so that the
**running system needs no internet**: a throwaway local chain (Anvil) instead of a
live chain, and **no Stripe** (billing disabled). You mint PKPs, create accounts,
groups, and actions, and run signing Lit Actions entirely against localhost.

> **Two phases, one internet boundary.**
> - **Install phase** (once) — downloading toolchains, crates, npm packages, the
>   dstack simulator, and the Chipotle source **requires the internet**.
> - **Run phase** (repeatable) — after a one-time "warm build", booting the stack
>   and running the verification flow **requires no internet**. No Stripe calls,
>   no live-chain RPC.

---

## What "runs offline" actually means

The stack is made of local-only pieces:

| Piece | Local substitute | Internet at runtime? |
|-------|------------------|----------------------|
| Blockchain | **Anvil** (`127.0.0.1:8545`), contracts deployed on boot | No |
| TEE / KMS keys | **dstack simulator** (unix socket) | No |
| Billing | **disabled** via `LIT_DISABLE_BILLING=true` | No (zero Stripe calls) |
| API | `lit-api-server` (`:8000`) | No |
| Action runner | `lit_actions` (unix socket) | No |
| Dashboard | `lit-static` via `static-web-server` (`:8080`) | No |
| Tracing (optional) | local Jaeger in Docker | No (auto-skipped if Docker is down) |

Caveats that keep the runtime offline:
- **Lit Actions that `import` from a CDN** (e.g. `esm.sh`) would need network. The
  verification actions in `lib/` import nothing — `ethers` is **bundled into the
  runtime as a global**, so they run fully offline.
- With billing disabled, all `/billing/*` endpoints return `"Billing not
  configured"`. That is expected, not a failure. Everything else works payment-free.

---

## TL;DR

```bash
# 0. Get these scripts (they live inside the Chipotle repo under local-runbook/).
#    On a brand-new machine, clone once to bootstrap, or copy local-runbook/ over.

# 1. Install every prerequisite (INTERNET). Then open a NEW shell.
bash local-runbook/scripts/00-install-prereqs.sh

# 2. Verify the prerequisites installed correctly.
bash local-runbook/scripts/01-check-prereqs.sh

# 3. Download the latest Chipotle (INTERNET).
bash local-runbook/scripts/02-fetch-chipotle.sh
export CHIPOTLE_DIR="$HOME/GitHub/chipotle"

# 4. Warm the build so the runtime is offline (INTERNET, one-time, slow).
bash local-runbook/scripts/03-warm-build.sh

# 5. Boot the stack — payment-free, local chain (OFFLINE-capable).
bash local-runbook/scripts/04-run-local.sh

# 6. Verify end-to-end: account creation, PKP mint, group/action setup, PKP signing.
bash local-runbook/scripts/05-verify-pkp-sign.sh

# 7. Stop everything.
bash local-runbook/scripts/06-teardown.sh
```

---

## Base requirements

Installed by `00-install-prereqs.sh`; verified by `01-check-prereqs.sh`.

| Tool | Why | macOS | Linux (Debian/Ubuntu) |
|------|-----|-------|-----------------------|
| **Rust** (`rustup`) + toolchain **1.91** | builds `lit-api-server`, `lit_actions`, `contract_deployer`. 1.91 is pinned by `rust-toolchain.toml` | rustup.rs | rustup.rs |
| **Foundry** (`anvil`, `forge`, `cast`) | Anvil = the local chain; `cast` provisions payers; `forge` binds contracts | foundryup | foundryup |
| **Node.js 20+** / npm | contract deploy runs `npm i` + `hardhat compile` | `brew install node` | NodeSource |
| **protobuf** (`protoc`) | `lit-actions-grpc` build script compiles `.proto` | `brew install protobuf` | `apt install protobuf-compiler` |
| **jq** | JSON parsing in the flow scripts | `brew install jq` | `apt install jq` |
| **static-web-server** | serves the dashboard | `brew install static-web-server` | official installer |
| **dstack simulator** | local TEE key derivation | cloned + built to `~/GitHub/dstack/sdk/simulator` | same |
| C build tools / `perl` / `make` / `curl` | Rust `-sys` crates, deploy scripting | Xcode CLT | `build-essential pkg-config libssl-dev clang` |
| **Docker** (optional) | Jaeger tracing only — auto-skipped if absent | `brew install --cask docker` | `apt install docker.io` |

> **Disk & memory.** The three Rust crates build into **separate** `target/` dirs
> (there is no shared workspace), so the warm build needs **~20 GB of free disk**
> and comfortably runs in **8 GB RAM**. If you run this inside Docker/a VM, make
> sure the VM's disk image has that much *free* — a full Docker disk shows up as
> `error: ... No space left on device (os error 28)` mid-build. Reclaim space with
> `docker builder prune -a` and `docker image prune` before blaming the runbook.

### Windows

Not supported natively — `local_test.sh` relies on Unix domain sockets and bash.
Use **WSL2 (Ubuntu)** and follow the Linux path inside it. Clone into the Linux
filesystem (`~/…`), not `/mnt/c/…`. Ports forward to Windows `localhost`
automatically.

---

## Phase-by-phase

### 1. Install prerequisites — `00-install-prereqs.sh` (internet)

Detects macOS vs Debian/Ubuntu and installs everything above, idempotently. On
macOS it installs Homebrew and Xcode Command Line Tools if missing (the CLT step
may pop a GUI dialog — accept it and re-run). It clones and builds the dstack
simulator into `~/GitHub/dstack/sdk/simulator` (override with `SIMULATOR_DIR`).

**Open a new shell afterward** so `PATH` changes (cargo, foundry) take effect.

### 2. Verify prerequisites — `01-check-prereqs.sh` (test)

Confirms each tool is present and prints versions, checks that the **1.91
toolchain resolves inside `lit-api-server`**, confirms the simulator binary
exists, and **boots a throwaway Anvil on `:8546` to prove the chain answers
JSON-RPC**. Exits non-zero if anything required is missing. Run it any time you
suspect a broken environment.

### 3. Download Chipotle — `02-fetch-chipotle.sh` (internet)

Clones `https://github.com/LIT-Protocol/chipotle.git` to `CHIPOTLE_DIR`
(default `~/GitHub/chipotle`) or `git pull`s an existing checkout. Override the
ref with `CHIPOTLE_REF` (default `main`). Prints the resolved HEAD SHA.

### 4. Warm the build — `03-warm-build.sh` (internet, one-time, slow)

Downloads and compiles everything the runtime needs so the *run* is offline:
Solidity deps (`npm i` + `hardhat compile`, caches solc), `contract_deployer`,
then the two long Rust builds (`lit-api-server`, `lit_actions`). Expect
10–20+ minutes and a few GB the first time.

### 5. Run locally — `04-run-local.sh` (offline-capable)

Exports `LIT_DISABLE_BILLING=true` and runs the repo's `local_test.sh`, which:
starts Anvil, starts the dstack simulator, deploys contracts + writes
`NodeConfig.toml`, provisions API payer accounts, then launches
`lit-api-server`, `lit_actions`, and the dashboard. The script runs the stack in
the **background**, logs to `/tmp/chipotle-local-stack.log`, and polls
`/core/v1/health` until ready. Use `FOREGROUND=1` to run it attached instead.

Health check when up:
```bash
curl -s http://localhost:8000/core/v1/health
# {"lit_actions_reachable":true,"lit_actions_gvisor_reachable":false,"cpu_available":true,"billing_keys_present":false}
```
`billing_keys_present:false` confirms Stripe is bypassed.
`lit_actions_gvisor_reachable:false` is expected off-TEE (gVisor is Linux/TEE-only)
and does not affect standard action execution.

### 6. Verify end-to-end — `05-verify-pkp-sign.sh` (test, offline)

The acceptance test. Requires the stack from step 5. Runs two flows and tallies
pass/fail:

- **Part A — action-key sign:** create account → create usage key → run a signing
  action that signs with the action's own CID-derived key.
- **Part B — PKP sign (the full lifecycle you asked for):**
  1. create account (`/new_account`)
  2. **mint PKP** (`/create_wallet`)
  3. compute action CID (`/get_lit_action_ipfs_id`)
  4. create group (`/add_group`)
  5. register action (`/add_action`)
  6. add action → group (`/add_action_to_group`)
  7. **add PKP → group** (`/add_pkp_to_group`) — authorizes CID→PKP
  8. create usage key that can **execute in that group** (`/add_usage_api_key`)
  9. run the action with `js_params.pkpId` (`/lit_action`)

  Then it **asserts the returned signer address equals the minted PKP address**.

Why the group wiring matters: `Lit.Actions.getPrivateKey({pkpId})` is a secret op.
The host authorizes it via the on-chain `canUseWalletInAction(actionCid, pkpId)`
point query, which passes **only** because both the action CID (steps 5–6) and the
PKP (step 7) belong to the same group. Drop step 6 or 7 and step 9 is denied.

### 7. Teardown — `06-teardown.sh`

Signals the launcher, kills any stray `anvil` / `dstack-simulator` /
`lit-api-server` / `lit_actions` / `static-web-server` processes, and removes the
Jaeger container if present.

---

## Proving the runtime is truly offline (optional)

After a successful `03-warm-build.sh` **and one online run** of step 5 (so npm/solc
caches are fully warm):

1. Tear down: `bash local-runbook/scripts/06-teardown.sh`
2. Disconnect the network (turn off Wi-Fi / pull the cable).
3. Boot again: `bash local-runbook/scripts/04-run-local.sh`
4. Verify: `bash local-runbook/scripts/05-verify-pkp-sign.sh`

Both should pass with no network. (The very first contract deploy caches solc; if
you never ran step 5 online, `hardhat compile` may still want to fetch solc once.)

---

## FAQ & gotchas (learned the hard way)

These are the failure modes we actually hit standing this up. Each is phrased as a
question so you can `Ctrl-F` the symptom.

**Q: A script dies immediately with `operation not permitted` or `failed to change
group ID`. Why?**
You ran it under **zsh**. zsh reserves the parameters `GID`/`UID`/`EUID`/`EGID`, and
assigning to them attempts a real `setgid()`/`setuid()`. Every script here uses
`#!/usr/bin/env bash` and avoids those names — invoke them with `bash`, not `zsh`.

**Q: `/billing/*` calls return `500 "Billing not configured"`. Is the build broken?**
No — that is expected with `LIT_DISABLE_BILLING=true`. Billing is intentionally off so
the runtime makes zero Stripe calls. The verify script's credit check tolerates it and
everything else works payment-free.

**Q: The health check shows `"lit_actions_gvisor_reachable":false`. Is that a
problem?**
No. gVisor is Linux/TEE-only; off-TEE it is expected to be unreachable. The standard
`lit_actions` runner (`"lit_actions_reachable":true`) is what these tests use, and it
does not affect action execution.

**Q: `04-run-local.sh` looks hung — nothing happens for minutes.**
The first boot compiles Rust. `04-run-local.sh` waits up to 30 minutes for health;
watch progress with `tail -f /tmp/chipotle-local-stack.log`.

**Q: `rustc --version` says 1.88 (or some other version), but the runbook needs 1.91.
Won't the build use the wrong compiler?**
No. `rust-toolchain.toml` pins **1.91** per crate, and `rustup` auto-selects it inside
the repo. The default toolchain on your machine is irrelevant. `01-check-prereqs.sh`
verifies the in-crate resolution.

**Q: Right after installing, I get `anvil: command not found` (or `cargo`, `forge`).**
Your current shell has a stale `PATH`. Open a **new shell**, or run
`export PATH="$HOME/.foundry/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"`.

**Q: `dstack-simulator not found`.**
It wasn't built, or it's in a non-default location. Re-run `00-install-prereqs.sh`, or
point `SIMULATOR_DIR` at your build. Default: `~/GitHub/dstack/sdk/simulator`.

**Q: Boot fails because ports 8000 / 8080 / 8545 are already in use.**
A previous stack is still running. Clear it with `bash local-runbook/scripts/06-teardown.sh`.

**Q: `03-warm-build.sh` fails with `error: ... No space left on device (os error 28)`.**
The disk (or the Docker VM disk) filled up. The three crates build into separate
`target/` dirs, so you need **~20 GB free**. Reclaim with `docker builder prune -a`
and `docker image prune`, or enlarge the Docker Desktop disk image, then re-run — the
build resumes where it stopped.

**Q: A Rust build fails with `Could not find `protoc``.**
protobuf isn't installed. Linux: `apt install protobuf-compiler`; macOS:
`brew install protobuf`. Then re-run `00-install-prereqs.sh` (or just re-run the build).

**Q: The `static-web-server` installer prints `cannot open html` / `Syntax error:
redirection unexpected`.**
The upstream install URL 301-redirects and a non-following `curl` pipes the HTML page
into `sh`. This runbook avoids that by downloading the arch-matched release binary
directly to `/usr/local/bin`. If you install it by hand, use `curl -L`.

**Q: After `06-teardown.sh` in a container, `pgrep` still lists `anvil` /
`lit-api-server` / `lit_actions`. Did teardown fail?**
No — check their state with `ps -o pid,stat,comm -p <pid>`: they'll be `Z` (zombie),
i.e. already dead, holding no ports or CPU. A container whose PID 1 is `sleep infinity`
never reaps orphans, so zombies linger until restart. Run the container with
`docker run --init` (adds a reaping init) to avoid them entirely. On a real host this
does not happen.

**Q: Can I install without `sudo` (e.g. as root in a container)?**
Yes. The installer detects root and skips `sudo` automatically (`asroot` helper).

---

## File reference

```
local-runbook/
├── README.md                     # this runbook
├── lib/
│   ├── pkp-sign.js               # action: signs with a MINTED PKP (getPrivateKey)
│   └── action-key-sign.js        # action: signs with the action's CID-derived key
└── scripts/
    ├── _common.sh                # shared helpers (colors, OS + repo detection, config)
    ├── 00-install-prereqs.sh     # install all base requirements (macOS/Linux)   [internet]
    ├── 01-check-prereqs.sh       # verify requirements + anvil smoke test        [test]
    ├── 02-fetch-chipotle.sh      # clone/update the Chipotle source              [internet]
    ├── 03-warm-build.sh          # pre-build so the runtime is offline           [internet]
    ├── 04-run-local.sh           # boot stack, payment-free, local chain         [offline-capable]
    ├── 05-verify-pkp-sign.sh     # end-to-end account + PKP-sign verification    [test, offline]
    └── 06-teardown.sh            # stop the stack and clean up
```

## Appendix: reproduce the full E2E in a clean Docker container

This runbook was validated end-to-end on a pristine `ubuntu:24.04` container
(arm64, 8 GB / 10 CPU). Use `--init` so the container reaps orphaned processes
(otherwise killed services linger as harmless zombies until restart), and make
sure the Docker VM has **~20 GB free** first.

```bash
# Free Docker VM disk if needed (regenerable cache; does not touch your images):
docker builder prune -a -f

# Clean container with an init that reaps zombies, runbook mounted read-only:
docker run -d --init --name chipotle-e2e \
  -v "$PWD/local-runbook":/runbook:ro ubuntu:24.04 sleep infinity

# Full flow inside the container:
docker exec chipotle-e2e bash /runbook/scripts/00-install-prereqs.sh
docker exec chipotle-e2e bash /runbook/scripts/01-check-prereqs.sh
docker exec chipotle-e2e bash /runbook/scripts/02-fetch-chipotle.sh
docker exec -e CHIPOTLE_DIR=/root/GitHub/chipotle chipotle-e2e bash /runbook/scripts/03-warm-build.sh
docker exec -e CHIPOTLE_DIR=/root/GitHub/chipotle chipotle-e2e bash /runbook/scripts/04-run-local.sh
docker exec -e CHIPOTLE_DIR=/root/GitHub/chipotle chipotle-e2e bash /runbook/scripts/05-verify-pkp-sign.sh

# Prove the runtime is air-gapped: cut the network, confirm it's really off, re-verify:
docker network disconnect bridge chipotle-e2e
docker exec chipotle-e2e bash -lc \
  'curl -sS --max-time 5 https://github.com >/dev/null 2>&1 && echo "STILL ONLINE (unexpected)" || echo "OFFLINE: external network unreachable ✅"'
docker exec -e CHIPOTLE_DIR=/root/GitHub/chipotle chipotle-e2e bash /runbook/scripts/05-verify-pkp-sign.sh
docker network connect bridge chipotle-e2e

docker exec chipotle-e2e bash /runbook/scripts/06-teardown.sh
docker rm -f chipotle-e2e
```

Result: `01` → 19/0, `05` → 12/0 (signer == minted PKP), and `05` passes again
with the network **disconnected** — confirming no Stripe, no live chain, no internet
at runtime.

## Appendix: environment variables

| Var | Default | Used by |
|-----|---------|---------|
| `CHIPOTLE_DIR` | auto-detect → `~/GitHub/chipotle` | all run/build/verify scripts |
| `CHIPOTLE_REPO` | `https://github.com/LIT-Protocol/chipotle.git` | `02-fetch` |
| `CHIPOTLE_REF` | `main` | `02-fetch` |
| `SIMULATOR_DIR` | `~/GitHub/dstack/sdk/simulator` | install, run |
| `API_PORT` / `DASH_PORT` / `CHAIN_PORT` | `8000` / `8080` / `8545` | run, verify |
| `FOREGROUND` | `0` | `04-run-local` (set `1` to run attached) |
| `TIMEOUT_SECS` | `1800` | `04-run-local` health wait |
| `STACK_LOG` | `/tmp/chipotle-local-stack.log` | run, teardown |

---

## Appendix: expected output

What a healthy run looks like. Captured from the validated clean-container E2E — your
addresses, PIDs, and CIDs will differ, but the shape and the ✅ lines should match.

### `01-check-prereqs.sh`

```
== Required command-line tools ==
   ✅ git  (/usr/bin/git)
   ✅ rustc  (/root/.cargo/bin/rustc)
   ✅ cargo  (/root/.cargo/bin/cargo)
   ✅ rustup  (/root/.cargo/bin/rustup)
   ✅ anvil  (/root/.foundry/bin/anvil)
   ✅ forge  (/root/.foundry/bin/forge)
   ✅ cast  (/root/.foundry/bin/cast)
   ✅ node  (/usr/bin/node)
   ✅ npm  (/usr/bin/npm)
   ✅ protoc  (/usr/bin/protoc)
   ✅ jq  (/usr/bin/jq)
   ✅ static-web-server  (/usr/local/bin/static-web-server)
   ✅ perl  (/usr/bin/perl)
   ✅ make  (/usr/bin/make)
   ✅ curl  (/usr/bin/curl)

== Rust toolchain 1.91 (pinned by rust-toolchain.toml) ==
   ✅ 1.91 toolchain installed
   ✅ inside lit-api-server, rustc resolves to: rustc 1.91.1 (ed61e7d7e 2025-11-07)

== dstack simulator ==
   ✅ simulator binary present at /root/GitHub/dstack/sdk/simulator/dstack-simulator

== Smoke test: can anvil boot and answer JSON-RPC? ==
   ✅ anvil answered eth_blockNumber on :8546

----- summary -----
   passed: 19   failed: 0

== All required prerequisites OK ==
```

### `04-run-local.sh` — stack up

```
== Stack is up ==
   chain (anvil):         http://127.0.0.1:8545
   lit-api-server:        http://localhost:8000
   dashboard:             http://localhost:8080
   health:                {"lit_actions_reachable":true,"lit_actions_gvisor_reachable":false,"cpu_available":true,"billing_keys_present":false}
```

`billing_keys_present:false` confirms Stripe is bypassed; `lit_actions_gvisor_reachable:false`
is expected off-TEE.

### `05-verify-pkp-sign.sh` — end-to-end PKP signing

```
== PART A — sign with the ACTION's own (CID-derived) key ==
   ✅ account created
   ✅ usage key created (execute_in_groups=[0])
   ✅ action ran: has_error=false, signer=0xABAaF9109898fCB803900BE6fca9aB5506E1fEa4

== PART B — sign with the MINTED PKP ==
== B1. create account ==
   ✅ account wallet=0x401627e2208e2ac4a0eb5cec6c01a82eb311ee3a
== B2. mint PKP (the signer) ==
   ✅ PKP=0x4def294a7038b87ede84445d42780ad209d6b9d6
== B3. compute action CID ==
   ✅ CID=Qma6rucZ1Ewen7yarNzyqQQW1PZoonQUuJYKPsDv8xswZm
== B4. create group ==
   ✅ group_id=1
== B5. register action ==
   ✅ add_action success
== B6. add action -> group ==
   ✅ add_action_to_group success
== B7. add PKP -> group (authorizes CID->PKP via canUseWalletInAction) ==
   ✅ add_pkp_to_group success
== B8. create usage key authorized to execute in group 1 ==
   ✅ usage key created
== B9. RUN the action, signing with the PKP (js_params.pkpId) ==
{
  "response": {
    "publicKey": "0x04ef57a3...",
    "signature": "0x0deb4b93...",
    "signer_wallet_address": "0x4DEF294a7038B87Ede84445D42780ad209d6B9d6"
  },
  "logs": "",
  "has_error": false
}

== Verification ==
   minted PKP address:      0x4def294a7038b87ede84445d42780ad209d6b9d6
   action signer address:   0x4DEF294a7038B87Ede84445D42780ad209d6B9d6
   has_error:               false
   ✅ MATCH — the signature was produced by the minted PKP

----- summary -----
   passed: 12   failed: 0

== LOCAL VERIFICATION PASSED ==
```

### Air-gap proof — `05` re-run with the network disconnected

First the manual reachability check (from the Docker appendix) confirms the network
is really off, then `05-verify-pkp-sign.sh` is re-run and still passes:

```
OFFLINE: external network unreachable ✅       # from the curl check, not the script
...
   ✅ MATCH — the signature was produced by the minted PKP
----- summary -----
   passed: 12   failed: 0
== LOCAL VERIFICATION PASSED ==
```

### `06-teardown.sh`

```
== Tearing down local Chipotle stack ==
   TERM: lit-api-server
   TERM: lit_actions
   TERM: anvil
   KILL: lit-api-server
   KILL: lit_actions
   KILL: anvil
   ✅ all stack processes stopped
   (3 zombie(s) awaiting reap by PID 1 — harmless; gone on container restart / use 'docker run --init')

== Done ==
```
