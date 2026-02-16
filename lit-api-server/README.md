# lit-peer-api-server

Rust API server for the Lit Protocol testnet. Exposes core Lit node operations (handshake, PKP mint/sign, encrypt/decrypt, Lit actions) plus higher-level **transfer** and **swap-intents** APIs. Serves static JavaScript SDKs and demo dApps.

## Quick start

- **Run the server** (from repo root; requires [lit-node-testnet](https://github.com/LIT-Protocol/lit-peer) and related crates in sibling paths):

  ```bash
  cargo run
  ```

  By default the API is at `http://localhost:8000` and static files (SDKs, dApps) are served from `/`.

- **Try the dApps**  
  Open `http://localhost:8000/dapps/swps/` (PKP wallet) or `http://localhost:8000/dapps/solver/` (swap solver). Configure the API URL in the footer if needed.

- **Blockchain tooling**  
  For generating Rust contract bindings from ABIs and deploying contracts, see [Blockchain → rust_generator_and_deployer](#blockchainrust_generator_and_deployer) below.

---

## Project structure

### `blockchain/`

Blockchain tooling and contracts used by the server.

| Path | Description |
|------|-------------|
| **`rust_generator_and_deployer/`** | Rust CLI tools: **contract generator** (ABI → Rust bindings) and **contract deployer** (artifact folder → chain). See [Usage for rust_generator_and_deployer](#blockchainrust_generator_and_deployer) below. |
| **`swaps/`** | Hardhat 3 project with the **QuoteStorage** Solidity contract (swap requests, quotes). Build with Hardhat; artifacts are used by the server’s swap abstractions. |

---

### `blockchain/rust_generator_and_deployer`

Rust tools for generating contract bindings and deploying contracts. Designed for experiments and automation.

- **Contract generator** — Reads ABI JSON files from a folder (recursively), uses `ethers::contract::Abigen` to generate Rust bindings with `serde::Serialize` / `Deserialize`, and writes `.rs` files plus copies of the ABI files into an output folder.
- **Contract deployer** — Reads a folder of contract artifact JSONs (Hardhat/Foundry-style: `abi` + `bytecode` or `evm.bytecode.object`), deploys each contract that has bytecode to a chosen network (Anvil, Yellowstone, or Lit Mainnet) using a built-in dev wallet.

**Usage**

From the `blockchain/rust_generator_and_deployer` directory:

```bash
# Build both binaries
cargo build --release
```

**1. Contract generator**

```bash
cargo run --release --bin contract_generator -- <input_folder> <output_folder>
```

- **`<input_folder>`** — Directory containing ABI JSON files (e.g. `../swaps/artifacts/contracts/quote.sol`, or a folder of multiple ABIs). Subfolders are processed recursively.
- **`<output_folder>`** — Directory where generated `.rs` files and copied ABI files are written (e.g. `../../src/abstractions/intents/swaps/contracts/` or any path you use in the server).

Example (generate from this repo’s swaps artifacts into the server’s swaps contracts folder):

```bash
cd blockchain/rust_generator_and_deployer
cargo run --release --bin contract_generator -- \
  ../swaps/artifacts/contracts/quote.sol \
  ../../src/abstractions/intents/swaps/contracts
```

**2. Contract deployer**

```bash
cargo run --release --bin contract_deployer -- <network> <abis_folder>
```

- **`<network>`** — `0` = Anvil, `1` = Yellowstone, `2` = Lit Mainnet.
- **`<abis_folder>`** — Folder containing contract **artifact** JSON files (must include `abi` and `bytecode` or `evm.bytecode.object`). Typically the compiled output from Hardhat (e.g. `../swaps/artifacts/build/` or the artifact JSONs under `artifacts/contracts/.../`).

Example (deploy swaps artifacts to local Anvil):

```bash
cd blockchain/rust_generator_and_deployer
# Ensure Anvil is running on 127.0.0.1:8545, then:
cargo run --release --bin contract_deployer -- 0 ../swaps/artifacts/contracts/quote.sol
```

**Deployer details**

- Uses a fixed dev wallet (Anvil account #0) for deployment; suitable for local/testnet only.
- Skips artifacts that have no bytecode (e.g. interfaces/libraries).
- RPC URLs: Anvil `http://127.0.0.1:8545`, Yellowstone/Lit Mainnet use `https://yellowstone-rpc.litprotocol.com`.

More detail and examples: [blockchain/rust_generator_and_deployer/README.md](blockchain/rust_generator_and_deployer/README.md).

---

### `static/` — JavaScript SDKs (3)

Client-side SDKs that call the server’s HTTP API. Served by the Rocket file server.

| File | Purpose | Server routes |
|------|--------|----------------|
| **`js_core_sdk.js`** | Core Lit node API: handshake, get API key, mint PKP, sign with PKP, encrypt/decrypt, combine signature shares, Lit actions, ledger balance. | `/core/v1/` |
| **`js_swaps_sdk.js`** | Swap intents: token list, new quote request, fill/accept quote, swap status, open requests/quotes, quote balances, attempt swap. | `/swaps/v1/` |
| **`js_transfer_sdk.js`** | Chain list (EVM/non-EVM, mainnet/testnet), balance by API key and chain, PKP native transfer. | `/transfer/v1/` |

API docs: `static/docs/` (server markdown and client HTML).

---

### `static/dapps/` — Demo apps (2)

| App | Path | Description |
|-----|------|-------------|
| **Solver** | `static/dapps/solver/` | **LIT Solver** — For solvers: list swap requests and quotes, commit to quote (MetaMask `newQuote`), process swap via `attempt_swap_request`. Uses Swaps SDK + ethers. |
| **Swps** | `static/dapps/swps/` | **LIT Swps** — PKP wallet: Overview (API key + PKP, balances), Transfer, Swap request, History. Uses Core, Transfer, and Swaps SDKs. |

Default API base URL: `http://localhost:8000` (configurable in the footer).

---

### `src/` — Core vs abstractions

- **`src/core/`** — **Core API** (Lit node primitives): handshake, get_api_key, mint_pkp, sign_with_pkp, encrypt, decrypt, combine_signature_shares, lit_action, get_ledger_balance. Mounted at `/core/v1/`; client: `js_core_sdk.js`.
- **`src/abstractions/`** — **Abstractions**
  - **`transfer/`** — Chain metadata, balance, PKP native transfer (EVM and non-EVM). Mounted at `/transfer/v1/`; client: `js_transfer_sdk.js`.
  - **`intents/swaps/`** — Swap intents (QuoteStorage contract, token list, quote lifecycle, attempt swap). Mounted at `/swaps/v1/`; client: `js_swaps_sdk.js`.

Route mounting and static file server are configured in `src/main.rs`.
