<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/logo/dark.svg" width="120">
    <source media="(prefers-color-scheme: light)" srcset="docs/logo/light.svg" width="120">
    <img alt="Lit Protocol" src="docs/logo/dark.svg" width="120">
  </picture>
</p>

<h1 align="center">Lit Chipotle</h1>

<p align="center">
  <strong>Programmable keys &middot; Verifiable compute &middot; One API call</strong>
</p>

<p align="center">
  Run JavaScript inside a TEE, sign with on-chain wallets, and produce cryptographic proofs&thinsp;&mdash;&thinsp;no private keys to manage, no nodes to run.
</p>

<p align="center">
  <a href="https://github.com/LIT-Protocol/chipotle/stargazers"><img src="https://img.shields.io/github/stars/LIT-Protocol/chipotle?style=social" alt="Stars"></a>&nbsp;
  <a href="https://api.dev.litprotocol.com/core/v1/swagger-ui"><img src="https://img.shields.io/badge/API-Swagger_UI-85ea2d" alt="Swagger"></a>&nbsp;
  <a href="https://dashboard.dev.litprotocol.com/dapps/dashboard/"><img src="https://img.shields.io/badge/Try-Dashboard-ff6b35" alt="Dashboard"></a>
</p>

---

## What is Chipotle?

Chipotle is the **Lit Express Node** — a single HTTP API backed by a TEE enclave, on-chain smart contracts, and IPFS-stored code. You send JavaScript, the enclave runs it with access to derived key material, and you get back signed, verifiable results.

Think of it as **serverless functions that can hold private keys**.

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

## Quickstart

Everything below works right now against the live dev API. No SDK needed — just `curl`.

### 1. Create an account

```bash
curl -s -X POST https://api.dev.litprotocol.com/core/v1/new_account \
  -H "Content-Type: application/json" \
  -d '{"account_name": "my-app", "account_description": "Getting started"}' | jq
```

```json
{
  "api_key": "T6j+7BAA…",
  "wallet_address": "0x3318…b0c5"
}
```

### 2. Create a wallet (PKP)

```bash
curl -s https://api.dev.litprotocol.com/core/v1/create_wallet \
  -H "X-Api-Key: $API_KEY" | jq
```

```json
{
  "wallet_address": "0x2a03…9bf6"
}
```

### 3. Run a Lit Action

```bash
curl -s -X POST https://api.dev.litprotocol.com/core/v1/lit_action \
  -H "X-Api-Key: $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "code": "async function main() { return { hello: \"world\", timestamp: Date.now() }; }"
  }' | jq
```

```json
{
  "response": { "hello": "world", "timestamp": 1711684200000 },
  "logs": "",
  "has_error": false
}
```

Or skip the terminal and use the **[Dashboard](https://dashboard.dev.litprotocol.com/dapps/dashboard/)** — a full GUI for account management, wallet creation, and action execution.

---

## What can you build?

Lit Actions are self-contained JavaScript programs that execute inside a TEE with access to derived keys.

```javascript
// A trustless oracle in 12 lines
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
<summary><strong>More patterns</strong></summary>

| Pattern | Description |
|---------|-------------|
| **Sign a message** | Retrieve a PKP key, sign arbitrary data, return a verifiable signature |
| **Encrypt / Decrypt** | Encrypt secrets to a PKP; only permitted actions can decrypt |
| **Gate on external data** | Fetch weather, stock prices, or any API — only sign if conditions are met |
| **Read smart contracts** | Call view functions on any EVM chain and sign the result as a proof |
| **Send ETH** | Construct, sign, and broadcast transactions from a PKP wallet |
| **Cross-chain swaps** | Swap intents via the Swaps API with quote lifecycle management |

</details>

---

## API surface

The server exposes three route groups. Every endpoint accepts `X-Api-Key` or `Authorization: Bearer <key>`.

| Group | Base path | Purpose |
|-------|-----------|---------|
| **Core** | `/core/v1/` | Accounts, wallets (PKPs), Lit Actions, groups, permissions, billing |
| **Transfer** | `/transfer/v1/` | Multi-chain balances and sends |
| **Swaps** | `/swaps/v1/` | Cross-chain intent-based trading (quotes, fills, status) |

Full OpenAPI spec: [`/core/v1/swagger-ui`](https://api.dev.litprotocol.com/core/v1/swagger-ui)

### Key endpoints

```
POST   /core/v1/new_account        Create an account → { api_key, wallet_address }
GET    /core/v1/create_wallet       Mint a new PKP wallet
POST   /core/v1/lit_action          Execute JavaScript in the TEE
POST   /core/v1/add_action          Register a Lit Action (IPFS CID or inline)
POST   /core/v1/add_group           Create a permission group
GET    /core/v1/list_wallets        List all PKP wallets for the account
GET    /core/v1/list_actions        List registered Lit Actions
GET    /core/v1/version             Server version and commit hash
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     YOUR APPLICATION                        │
│              REST API  ·  JS SDK  ·  Dashboard              │
└──────────────────────────┬──────────────────────────────────┘
                           │ HTTPS
┌──────────────────────────▼──────────────────────────────────┐
│                    TEE ENCLAVE (Phala)                       │
│                                                             │
│  ┌─────────────┐    gRPC    ┌──────────────────────┐       │
│  │  API Server  │◄─────────►│  Lit Actions Runtime  │       │
│  │   (Rocket)   │           │   (Deno sandbox)     │       │
│  └──────┬───────┘           └──────────┬───────────┘       │
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

## Running locally

```bash
cd lit-api-server && cargo run
# API → http://localhost:8000
# Swagger → http://localhost:8000/core/v1/swagger-ui
```

See [lit-api-server/README.md](lit-api-server/README.md) for configuration and deployment details.

---

## Links

| | |
|---|---|
| **Documentation** | [`docs/`](docs/) |
| **Dashboard** | [dashboard.dev.litprotocol.com](https://dashboard.dev.litprotocol.com/dapps/dashboard/) |
| **API** | [api.dev.litprotocol.com](https://api.dev.litprotocol.com/) |
| **OpenAPI / Swagger** | [Swagger UI](https://api.dev.litprotocol.com/core/v1/swagger-ui) |
| **Architecture** | [Architecture overview](docs/architecture/index.mdx) |
| **Lit Actions examples** | [Examples guide](docs/lit-actions/examples.mdx) |
| **Lit Protocol** | [litprotocol.com](https://litprotocol.com) |

---

## Contributing

We welcome contributions. Please open an issue to discuss your idea before submitting a PR.

## License

All rights reserved. Copyright Lit Protocol.
