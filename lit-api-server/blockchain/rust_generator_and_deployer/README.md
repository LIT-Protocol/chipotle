# rust_generator_and_deployer

Rust CLI tool for deploying Lit blockchain contract artifacts to a chain (Anvil, Yellowstone, Base Sepolia, or Base).

The old `contract_generator` binary was removed after contract binding generation moved to `forge bind` in `blockchain/lit_node_express`.

## Build

From this directory:

```bash
cargo build --release
```

Binary: `target/release/contract_deployer`.

---

## Contract deployer

Deploys contracts from a folder of **artifact** JSON files (Hardhat/Foundry style: `abi` + `bytecode` or `evm.bytecode.object`) to a selected network. Uses a built-in dev wallet (Anvil account #0) unless `--secret` is provided; intended for local and testnet use only.

### Usage

```text
cargo run --release --bin contract_deployer -- \
  --action=<deploy|update|propose-update> \
  --network=<anvil|yellowstone|base-sepolia|base> \
  --abifolder=<artifacts_folder> \
  [--secret=<private_key>] \
  [--address=<diamond_address>] \
  [--output=<proposal_json_path>] \
  [--rpc-url=<custom_rpc_url>]
```

| Argument | Description |
|----------|-------------|
| `--action` | `deploy` deploys a fresh diamond; `update` updates an existing diamond directly; `propose-update` writes diamond-cut calldata for multisig tooling. |
| `--network` | `anvil`, `yellowstone`, `base-sepolia`, or `base`. |
| `--abifolder` | Folder containing contract artifact JSON files. |
| `--secret` | Optional deployer private key (hex). If omitted or blank, uses Anvil account #0. |
| `--address` | Existing diamond address; required for `update` and `propose-update`. |
| `--output` | Proposal JSON output path for `propose-update` (default: `diamond_cut_proposal.json`). |
| `--rpc-url` | Optional RPC URL override. |

### Notes

- The deployer uses Alloy for provider, signer, deployment, ABI selector extraction, and calldata encoding.
- JSON artifacts under `src/diamond/*.json` are preserved because Rust deployer code and `lit_node_express` helpers/tests load them directly.
- Fresh deploys use checked-in diamond foundation artifacts from `src/diamond/*.json` plus app facet artifacts from `--abifolder`. Updates/proposals deploy app facets from `--abifolder` and OwnershipFacet from the checked-in artifact.
