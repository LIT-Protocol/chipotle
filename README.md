<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/logo/dark.svg" width="120">
    <source media="(prefers-color-scheme: light)" srcset="docs/logo/light.svg" width="120">
    <img alt="Lit Protocol" src="docs/logo/dark.svg" width="120">
  </picture>
</p>

<h1 align="center">Lit Chipotle</h1>

<p align="center">
  <strong>Programmable key management &middot; Verifiable compute &middot; One API call</strong>
</p>

<p align="center">
  Run JavaScript inside a TEE, sign with network-managed wallets, and return cryptographically verifiable results&thinsp;&mdash;&thinsp;no private keys to manage, no nodes to run.
</p>

<p align="center">
  <a href="https://github.com/LIT-Protocol/chipotle/stargazers"><img src="https://img.shields.io/github/stars/LIT-Protocol/chipotle?style=social" alt="Stars"></a>&nbsp;
  <a href="https://api.dev.litprotocol.com/core/v1/swagger-ui"><img src="https://img.shields.io/badge/API-Swagger_UI-85ea2d" alt="Swagger"></a>&nbsp;
  <a href="https://dashboard.dev.litprotocol.com/dapps/dashboard/"><img src="https://img.shields.io/badge/Try-Dashboard-ff6b35" alt="Dashboard"></a>&nbsp;
  <a href="https://docs.dev.litprotocol.com/"><img src="https://img.shields.io/badge/Docs-docs.dev.litprotocol.com-blue" alt="Docs"></a>
</p>

---

## What is Chipotle?

Chipotle is a managed service providing a REST API and web dashboard for programmable key management. It comprises three composable layers:

1. **TEE Enclave** — holds the root key, derives signing and encryption keys on demand, and executes sandboxed JavaScript. Nothing that touches key material ever leaves the enclave.
2. **On-Chain Permissions (Base)** — all authorization state lives on-chain: accounts, API key scopes, PKP registrations, and groups.
3. **Lit Actions (IPFS)** — immutable JavaScript programs stored by content ID (CID). Public, content-addressed, tamper-proof.

Think of it as **serverless functions that can hold private keys**.

<table>
<tr>
<td width="50%">

### For Web3 developers
Write a Lit Action in plain JavaScript. It can sign transactions, encrypt and decrypt secrets, read on-chain state, fetch external APIs, and return cryptographically signed proofs — all governed by on-chain permission groups.

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

Lit Actions are immutable JavaScript programs stored on IPFS and executed inside the TEE with access to derived keys. They can sign data, encrypt and decrypt secrets, make HTTP requests, and return cryptographically attested results.

```javascript
// Fetch a live price and sign it as a verifiable proof
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
| **Encrypt a secret** | Secure sensitive data using the PKP for storage anywhere |
| **Decrypt a secret** | Recover plaintext from previously encrypted data using the same PKP |
| **Gate on external data** | Fetch weather, prices, or any API — only sign if conditions are met |
| **Read smart contracts** | Call view functions on any EVM chain and sign the result as a proof |
| **Send ETH** | Construct, sign, and broadcast transactions from a PKP wallet |

See the full [Examples guide](https://docs.dev.litprotocol.com/lit-actions/examples) for copy-paste code.

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
│                    TEE ENCLAVE (Phala / dstack)              │
│                                                             │
│  ┌─────────────┐    gRPC    ┌──────────────────────┐       │
│  │  API Server  │◄─────────►│  Lit Actions Runtime  │       │
│  │   (Rocket)   │           │   (Deno sandbox)     │       │
│  └──────┬───────┘           └──────────┬───────────┘       │
│         │                              │                    │
│    Root key + derived             Executes JS with          │
│    keys never leave               derived key material      │
│    the enclave                                              │
└─────────┼──────────────────────────────┼────────────────────┘
          │ read/write permissions       │ fetch code
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

**TEE Enclave** — Holds the root key, derives signing and encryption keys on demand, and runs sandboxed JavaScript. Key material only exists transiently inside the enclave.
**On-Chain (Base)** — Account contracts, API key registries, PKP registries, and group permission policies. All verifiable, all on-chain.
**IPFS** — Lit Actions stored by content hash. Not owned by anyone — public, content-addressed code.

> The system treats "self-sovereign" and "SaaS" as configurations of the same architecture, not distinct modes. The difference is who owns the Account contract and what scopes the API keys carry. [Learn more](https://docs.dev.litprotocol.com/architecture/authModel)

---

## Features

| | |
|---|---|
| **Programmable Key Pairs (PKPs)** | Network-managed elliptic-curve key pairs. Key material is derived on-demand from the root key inside the TEE — it never exists at rest. |
| **Lit Actions** | Immutable JavaScript programs on IPFS. They can sign, encrypt, decrypt, fetch external data, and call smart contracts. |
| **Groups** | Permission policies binding PKPs to action CIDs and scoped API keys. Controls both *what* can execute and *who* can trigger it. |
| **Encrypt / Decrypt** | PKP-derived symmetric encryption. Store ciphertexts anywhere — only permitted actions can decrypt. |
| **On-Chain Permissions** | Smart contracts on Base control accounts, API key scopes, PKP registrations, and group membership. |
| **Cross-Chain Transfers & Swaps** | Transfer API for multi-chain sends. Swaps API for cross-chain intent-based trading. |
| **REST + SDK + Dashboard** | Three ways in: raw HTTP, the [Core SDK](https://docs.dev.litprotocol.com/management/api_direct), or the [Dashboard](https://docs.dev.litprotocol.com/management/dashboard). |
| **Verifiable Deployment** | TEE attestation + on-chain state = cryptographic proof the node is running expected code. [Verification guide](https://docs.dev.litprotocol.com/architecture/verification/index) |

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
| **Documentation** | [docs.dev.litprotocol.com](https://docs.dev.litprotocol.com/) |
| **Dashboard** | [dashboard.dev.litprotocol.com](https://dashboard.dev.litprotocol.com/dapps/dashboard/) |
| **API** | [api.dev.litprotocol.com](https://api.dev.litprotocol.com/) |
| **OpenAPI / Swagger** | [Swagger UI](https://api.dev.litprotocol.com/core/v1/swagger-ui) |
| **Architecture** | [Architecture overview](https://docs.dev.litprotocol.com/architecture/index) |
| **Auth model** | [Authentication model](https://docs.dev.litprotocol.com/architecture/authModel) |
| **Lit Actions** | [Overview](https://docs.dev.litprotocol.com/lit-actions/index) · [Examples](https://docs.dev.litprotocol.com/lit-actions/examples) · [Patterns](https://docs.dev.litprotocol.com/lit-actions/patterns) |
| **Pricing** | [Credit-based pricing](https://docs.dev.litprotocol.com/management/pricing) |
| **Lit Protocol** | [litprotocol.com](https://litprotocol.com) |

---

## Contributing

We welcome contributions. Please open an issue to discuss your idea before submitting a PR.

## License

All rights reserved. Copyright Lit Protocol.
