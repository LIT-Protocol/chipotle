# rust_generator_and_deployer

Rust CLI tools for the Lit blockchain stack: generate Rust contract bindings from ABI files, and deploy contract artifacts to a chain (Anvil, Yellowstone, Base Sepolia, or Base).

## Build

From this directory:

```bash
cargo build --release
```

Binaries: `target/release/contract_generator`, `target/release/contract_deployer`.

---

## 1. Contract generator

Generates Rust contract bindings (using `ethers::contract::Abigen`) from ABI JSON files. Outputs `.rs` modules with `serde::Serialize` and `serde::Deserialize` derives, and copies the source ABI files into the output folder.

### Usage

```text
cargo run --release --bin contract_generator -- <input_folder> <output_folder>
```

| Argument | Description |
|----------|-------------|
| `input_folder` | Folder containing ABI JSON files (e.g. Hardhat artifact paths like `../swaps/artifacts/contracts/quote.sol`). Subdirectories are processed recursively. |
| `output_folder` | Directory where generated `.rs` files and copied ABI files are written. Created if it does not exist. |

### Examples

Generate from this repo’s QuoteStorage artifacts into the API server’s swaps contracts module:

```bash
# From blockchain/rust_generator_and_deployer
cargo run --release --bin contract_generator -- \
  ../swaps/artifacts/contracts/quote.sol \
  ../../src/abstractions/intents/swaps/contracts
```

Generate from a generic ABI directory:

```bash
cargo run --release --bin contract_generator -- ./abis ./generated
```

### Input format

- Each file should be a JSON ABI (array of function/event/error items) or a path to such a file.
- The generator uses `Abigen::from_file()`; filenames without `.json` are fine as long as the content is valid ABI JSON.
- If a file fails to parse as an ABI, an error is printed and the tool continues with the next file.

### Output

- One `.rs` file per successful ABI, named from the generated module name (e.g. `QuoteStorage.rs`).
- A copy of each source ABI file is placed in `output_folder` for reference.

---

## 2. Contract deployer

Deploys contracts from a folder of **artifact** JSON files (Hardhat/Foundry style: `abi` + `bytecode` or `evm.bytecode.object`) to a selected network. Uses a built-in dev wallet (Anvil account #0); intended for local and testnet use only.

### Usage

```text
cargo run --release --bin contract_deployer -- <network> <abis_folder>
```

| Argument | Description |
|----------|-------------|
| `network` | `0` = Anvil (local), `1` = Yellowstone, `2` = Base Sepolia, `3` = Base. |
| `abis_folder` | Folder containing contract artifact JSON files. Subdirectories are scanned. Each artifact must have `abi` and either top-level `bytecode` or `evm.bytecode.object`. |

### Networks

| Code | Network | RPC |
|------|---------|-----|
| `0` | Anvil | `http://127.0.0.1:8545` (chain id 31337) |
| `1` | Yellowstone | `https://yellowstone-rpc.litprotocol.com` (chain id 175188) |
| `2` | Base Sepolia | `https://sepolia.base.org` (chain id 84532) |
| `3` | Base | `https://mainnet.base.org` (chain id 8453) |

### Examples

Deploy this repo’s swaps contract to local Anvil (start Anvil first, e.g. `anvil`):

```bash
# From blockchain/rust_generator_and_deployer
cargo run --release --bin contract_deployer -- 0 ../swaps/artifacts/contracts/quote.sol
```

Deploy to Yellowstone testnet:

```bash
cargo run --release --bin contract_deployer -- 1 ../swaps/artifacts/contracts/quote.sol
```

Deploy from a flat folder of artifact JSONs:

```bash
cargo run --release --bin contract_deployer -- 0 ./artifacts
```

### Behavior

- **Wallet:** Uses the default Anvil account #0 private key (`0xac0974...f80`). Do not use for mainnet or real funds.
- **Artifacts:** Only files that contain non-empty bytecode are deployed; interfaces and libraries (no bytecode) are skipped with a message.
- **Order:** Deployments run in the order files are discovered (directory order). No dependency ordering is applied; ensure deploy order matches contract dependencies if needed.

### After deployment

The deployer prints the deployed contract addresses. Configure the API server (e.g. swap abstractions) with the QuoteStorage address for the chain you deployed to.
