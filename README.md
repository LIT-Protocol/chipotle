<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/logo/dark.svg" width="120">
    <source media="(prefers-color-scheme: light)" srcset="docs/logo/light.svg" width="120">
    <img alt="Lit Protocol" src="docs/logo/dark.svg" width="120">
  </picture>
</p>

<h1 align="center">Lit Chipotle</h1>

<p align="center">
  <strong>Programmable keys. Verifiable compute. One API call.</strong>
</p>

<p align="center">
  Run JavaScript inside a TEE, sign with on-chain wallets, and produce cryptographic proofs —<br>
  no private keys to manage, no nodes to run.
</p>

<p align="center">
  <a href="https://github.com/LIT-Protocol/chipotle/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="License"></a>
  <a href="https://github.com/LIT-Protocol/chipotle/stargazers"><img src="https://img.shields.io/github/stars/LIT-Protocol/chipotle?style=social" alt="Stars"></a>
  <a href="https://api.dev.litprotocol.com/swagger-ui/"><img src="https://img.shields.io/badge/API-Swagger_UI-85ea2d" alt="Swagger"></a>
  <a href="https://dashboard.dev.litprotocol.com/dapps/dashboard/"><img src="https://img.shields.io/badge/Try-Dashboard-ff6b35" alt="Dashboard"></a>
</p>

---

## What is Chipotle?

Chipotle is Lit Protocol's express node — a single HTTP endpoint backed by a **TEE enclave**, **on-chain smart contracts**, and **IPFS-stored code**. You send JavaScript, the enclave runs it with access to derived key material, and you get back signed, verifiable results. Think of it as **serverless functions that can hold private keys**.

<table>
<tr>
<td width="50%">

### For Web3 developers
Write a Lit Action in plain JavaScript. It can sign transactions, encrypt secrets, read on-chain state, fetch APIs, and produce cryptographic proofs — all enforced by on-chain permissions.

</td>
<td width="50%">

### For traditional developers
A REST API with a JS SDK. Create an account, get an API key, call one endpoint. No wallets, no MetaMask, no Solidity required.

</td>
</tr>
</table>

---

## Zero to Lit Action in 60 seconds

```bash
# 1. Create an account
curl -s https://api.dev.litprotocol.com/core/v1/handshake | jq

# 2. Run a Lit Action — sign a message with a PKP wallet
curl -X POST https://api.dev.litprotocol.com/core/v1/lit_action \
  -H "X-Api-Key: YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "code": "async function main({ pkpId, message }) { const wallet = new ethers.Wallet(await Lit.Actions.getPrivateKey({ pkpId })); return { signature: await wallet.signMessage(message) }; }",
    "js_params": { "pkpId": "YOUR_PKP_ADDRESS", "message": "Hello from Chipotle!" }
  }'
```

Or skip the terminal and use the **[Dashboard](https://dashboard.dev.litprotocol.com/dapps/dashboard/)** — a full GUI for account management, wallet creation, and action execution.

---

## What can you build?

Lit Actions are self-contained JavaScript programs that execute inside a TEE with access to derived keys. Here's a taste:

```javascript
// Fetch a live price, sign it as a verifiable proof — a trustless oracle in 12 lines
async function main({ pkpId }) {
  const res = await fetch(
    "https://api.coingecko.com/api/v3/simple/price?ids=ethereum&vs_currencies=usd"
  );
  const price = (await res.json())?.ethereum?.usd;

  const wallet = new ethers.Wallet(
    await Lit.Actions.getPrivateKey({ pkpId })
  );
  const signature = await wallet.signMessage(`ETH/USD: ${price}`);

  return { price, signature };
}
```

A smart contract can `ecrecover` the signature to confirm the price was attested by a known PKP — **no off-chain trust required**.

<details>
<summary><strong>More examples</strong></summary>

| Pattern | Description |
|---------|-------------|
| **Sign a message** | Retrieve a PKP key, sign arbitrary data, return a verifiable signature |
| **Encrypt / Decrypt** | Encrypt secrets to a PKP; only permitted actions can decrypt |
| **Gate on external data** | Fetch weather, stock prices, or any API — only sign if conditions are met |
| **Read smart contracts** | Call view functions on any EVM chain and sign the result as a proof |
| **Send ETH** | Construct, sign, and broadcast transactions from a PKP wallet |
| **Cross-chain swaps** | Swap intents via the Swaps API with quote lifecycle management |

See the full [Examples guide](docs/lit-actions/examples.mdx) for copy-paste code.

</details>

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     YOUR APPLICATION                        │
│              REST API  ·  JS SDK  ·  Dashboard               │
└──────────────────────────┬──────────────────────────────────┘
                           │ HTTPS
┌──────────────────────────▼──────────────────────────────────┐
│                    TEE ENCLAVE (Phala)                       │
│                                                             │
│  ┌─────────────┐    gRPC    ┌──────────────────────┐        │
│  │  API Server  │◄─────────►│  Lit Actions Runtime  │        │
│  │   (Rocket)   │           │   (Deno sandbox)     │        │
│  └──────┬───────┘           └──────────┬───────────┘        │
│         │                              │                    │
│    Root key never                Executes JS with           │
│    leaves enclave               derived key material        │
└─────────┼──────────────────────────────┼────────────────────┘
          │ read permissions             │ fetch code
          ▼                              ▼
   ┌──────────────┐              ┌──────────────┐
   │  BASE Chain   │              │     IPFS     │
   │               │              │              │
   │  Accounts     │              │  Lit Actions │
   │  API Keys     │              │  (by CID)   │
   │  PKPs         │              │  Immutable   │
   │  Groups       │              │  Reusable    │
   └──────────────┘              └──────────────┘
```

**TEE Enclave** — Holds root keys, runs sandboxed JavaScript, never exposes key material.
**On-Chain (Base)** — Accounts, API key scopes, PKP registrations, and group permissions — all verifiable, all immutable.
**IPFS** — Lit Actions stored by content hash. Public, reusable, tamper-proof.

> The security model is emergent: a 3-of-5 multisig on the Account contract gives you self-sovereign control. A Stytch-authenticated account gives you SaaS convenience. Same API, same code, different trust assumptions. [Learn more](docs/architecture/index.mdx)

---

## Features

| | |
|---|---|
| **Programmable Key Pairs (PKPs)** | On-chain registered wallets whose keys live inside the TEE. Sign anything, from any chain. |
| **Sandboxed Execution** | Lit Actions run in a Deno V8 sandbox with memory limits, timeouts, and zero ambient authority. |
| **On-Chain Permissions** | Smart contracts on Base control who can use which keys and which actions. Auditable and upgradeable. |
| **Encrypt / Decrypt** | PKP-scoped encryption — store ciphertexts anywhere, only permitted actions can decrypt. |
| **Cross-Chain Transfers & Swaps** | Transfer API for multi-chain sends. Swaps API for cross-chain intent-based trading. |
| **OpenTelemetry Observability** | Built-in tracing and metrics. Plug in Jaeger, Grafana, or any OTEL-compatible backend. |
| **REST + SDK + Dashboard** | Three ways in: raw HTTP, lightweight JS SDK, or a full management GUI. |
| **Verifiable Deployment** | TEE attestation + on-chain state = cryptographic proof the node is running expected code. |

---

## Project structure

```
lit-core/          Shared Rust libraries (config, encoding, API primitives, logging, observability)
lit-actions/       Deno-based JS execution engine, gRPC interface, CLI, tests
lit-api-server/    Rocket HTTP server — Core, Transfer, and Swaps APIs
lit-static/        JavaScript SDKs and demo dApps (Dashboard, SWPS Wallet, Solver)
k6/                Load tests, security tests, correctness tests
docs/              Full documentation site (Mintlify)
```

See [lit-api-server/README.md](lit-api-server/README.md) and [lit-actions/README.md](lit-actions/README.md) for detailed module documentation.

---

## Running locally

```bash
# Build and start the API server (Rust toolchain required)
cd lit-api-server && cargo run

# The API is live at http://localhost:8000
# Swagger UI at http://localhost:8000/swagger-ui/
```

For the full stack (API server + Lit Actions runtime), see the [Deployment guide](docs/deployment).

---

## Links

| | |
|---|---|
| **Documentation** | [`docs/`](docs/) |
| **Dashboard** | [dashboard.dev.litprotocol.com](https://dashboard.dev.litprotocol.com/dapps/dashboard/) |
| **API** | [api.dev.litprotocol.com](https://api.dev.litprotocol.com/) |
| **OpenAPI / Swagger** | [Swagger UI](https://api.dev.litprotocol.com/swagger-ui/) |
| **Architecture** | [Architecture overview](docs/architecture/index.mdx) |
| **Lit Actions examples** | [7 patterns](docs/lit-actions/examples.mdx) |
| **Lit Protocol** | [litprotocol.com](https://litprotocol.com) |

---

## Contributing

We welcome contributions. Please open an issue to discuss your idea before submitting a PR.

## License

Apache-2.0 — see [LICENSE](LICENSE)
