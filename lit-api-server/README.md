# lit-api-server

Rust API server for the Lit Express Node. Exposes core Lit node operations (account config, PKP mint/sign, encrypt/decrypt, Lit actions) plus **transfer** and **swap-intents** APIs. Serves static JavaScript SDKs and demo dApps.

## Quick start

1. **Configure the node**  
   Copy `NodeConfig.sample.toml` to `NodeConfig.toml` in the `lit-api-server` directory and set your chain (e.g. `anvil`), AccountConfig contract address, and deployer secret (private key):

   ```toml
   [chain]
   name = "anvil"
   contract_address = "0x..."
   secret = "0x..."
   ```

   The server reads this file at startup and will exit with an error if it is missing or invalid.

2. **Run the server** (from the `lit-api-server` directory or repo root):

   ```bash
   cargo run
   ```

   By default the API is at `http://localhost:8000` and static files (SDKs, dApps) are served from `/`.

3. **Try the dashboard**  
   Open `http://localhost:8000/dapps/dashboard/` to manage account config: groups, usage API keys, IPFS actions, wallets, and run Lit actions. Log in with an API key or create a new account.

4. **Other dApps**  
   `http://localhost:8000/dapps/swps/` (PKP wallet), `http://localhost:8000/dapps/solver/` (swap solver). Set the API URL in the footer if needed.

5. **Blockchain tooling**  
   For generating Rust contract bindings and deploying contracts, see [Blockchain → rust_generator_and_deployer](#blockchainrust_generator_and_deployer) below.

---

## Configuration

**NodeConfig.toml** (required in the working directory when running the server):

| Field | Description |
|-------|-------------|
| `chain.name` | Chain for the AccountConfig contract (e.g. `anvil`) |
| `chain.contract_address` | Deployed AccountConfig contract address |
| `chain.secret` | Private key (hex) for the account config deployer/signer |

See `NodeConfig.sample.toml` for an example.

---

## Project structure

### `blockchain/`

Blockchain tooling and contracts used by the server.

| Path | Description |
|------|-------------|
| **`rust_generator_and_deployer/`** | Rust CLI tools: **contract generator** (ABI → Rust bindings) and **contract deployer** (artifact folder → chain). See [Usage](#blockchainrust_generator_and_deployer) below. |
| **`lit_node_express/`** | Lit Node Express contracts and Makefile. |
| **`swaps/`** | Hardhat project with the **QuoteStorage** Solidity contract (swap requests, quotes). |

---

### `blockchain/rust_generator_and_deployer`

Rust tools for generating contract bindings and deploying contracts.

- **Contract generator** — Reads ABI JSON files from a folder (recursively), uses `ethers::contract::Abigen` to generate Rust bindings, and writes `.rs` files and ABI copies into an output folder.
- **Contract deployer** — Reads a folder of contract artifact JSONs (Hardhat/Foundry-style: `abi` + `bytecode` or `evm.bytecode.object`), deploys each contract that has bytecode to a chosen network using a configurable or default wallet.

**Build (from `blockchain/rust_generator_and_deployer`):**

```bash
cargo build --release
```

**1. Contract generator**

```bash
cargo run --release --bin contract_generator -- <input_folder> <output_folder>
```

- **`<input_folder>`** — Directory containing ABI JSON files (subfolders processed recursively).
- **`<output_folder>`** — Where generated `.rs` files and copied ABIs are written.

**2. Contract deployer**

```bash
cargo run --release --bin contract_deployer -- <network> <abis_folder> [secret]
```

- **`<network>`** — `0` = Anvil, `1` = Yellowstone, `2` = Base Sepolia.
- **`<abis_folder>`** — Folder of contract **artifact** JSONs (`abi` + `bytecode` or `evm.bytecode.object`).
- **`[secret]`** — Optional. Deployer private key (hex). If omitted or blank, uses the default Anvil dev secret.

Example (deploy to local Anvil with default secret):

```bash
cd blockchain/rust_generator_and_deployer
# Ensure Anvil is running on 127.0.0.1:8545, then:
cargo run --release --bin contract_deployer -- 0 ../swaps/artifacts/contracts/quote.sol
```

Example (deploy with a custom key):

```bash
cargo run --release --bin contract_deployer -- 0 ./artifacts 0xYourPrivateKeyHex
```

**Deployer details**

- Uses the given secret or the default Anvil account #0 key; suitable for local/testnet.
- Skips artifacts with no bytecode (e.g. interfaces).
- RPC URLs: Anvil `http://127.0.0.1:8545`, Yellowstone `https://yellowstone-rpc.litprotocol.com`, Base Sepolia `https://sepolia.base.org`.

---

### `static/` — JavaScript SDKs

Client SDKs that call the server’s HTTP API. Served by the Rocket file server.

| File | Purpose | Server routes |
|------|--------|----------------|
| **`core_sdk.js`** | Core API: account (new, exists), create wallet, sign with PKP, Lit actions, AccountConfig (groups, usage keys, actions, PKPs). | `/core/v1/` |
| **`transfer_sdk.js`** | Chain list, balance by API key/PKP/address, send. | `/transfer/v1/` |
| **`swaps_sdk.js`** | Swap intents: token list, quote request, fill/accept quote, swap status, open requests/quotes. | `/swaps/v1/` |

API docs: `static/docs/` (server markdown and client HTML).

---

### `static/dapps/` — Demo apps

| App | Path | Description |
|-----|------|-------------|
| **Dashboard** | `/dapps/dashboard/` | **Lit Express Node Dashboard** — Log in with API key or create account; manage Usage API keys, Groups, IPFS Actions, Wallets; run Lit actions and get Lit Action IPFS CID. |
| **Swps** | `/dapps/swps/` | **LIT Swps** — PKP wallet: overview, transfer, swap request, history. Uses Core, Transfer, and Swaps SDKs. |
| **Solver** | `/dapps/solver/` | **LIT Solver** — For solvers: list swap requests/quotes, commit to quote, attempt swap. |

Default API base URL: `http://localhost:8000` (configurable in the footer where applicable).

---

### `src/` — Core vs abstractions

- **`src/core/`** — **Core API**: account (new_account, account_exists), create_wallet, sign_with_pkp, lit_action, get_lit_action_ipfs_id; AccountConfig operations (add/update/remove group, usage keys, actions, list groups/wallets/actions). Mounted at `/core/v1/`; client: `core_sdk.js`.
- **`src/config.rs`** — Loads `NodeConfig.toml` (chain, contract_address, secret) at startup.
- **`src/accounts/`** — AccountConfig contract integration (groups, usage keys, actions, PKPs).
- **`src/actions/`** — Lit action execution (gRPC, JS runtime).
- **`src/abstractions/`**
  - **`transfer/`** — Chain metadata, balance, send. Mounted at `/transfer/v1/`; client: `transfer_sdk.js`.
  - **`intents/swaps/`** — Swap intents (QuoteStorage, token list, quote lifecycle). Mounted at `/swaps/v1/`; client: `swaps_sdk.js`.

Route mounting and static file server are in `src/main.rs`.
