---
name: lit-chipotle-api
description: Teaches how to create and configure a Lit Chipotle account (accounts, groups, wallets, actions, API keys), write and run Lit Actions, and use the Core/Transfer/Swaps APIs. Use when an AI needs to set up an account, manage resources, write a Lit Action, or call any API endpoint.
---

# Lit Chipotle API — Complete guide for AIs

Lit Chipotle is a programmable key management system. Accounts hold wallets (PKPs), IPFS-pinned JavaScript programs (Lit Actions), and permission groups — all enforced on-chain via the `AccountConfig` smart contract and executed inside a TEE (Trusted Execution Environment).

**Production API**: `https://api.chipotle.litprotocol.com`
**Dev API**: `https://api.dev.litprotocol.com`
**Dashboard**: `https://dashboard.chipotle.litprotocol.com/dapps/dashboard/`
**Local dev**: `http://localhost:8000`

Routes are mounted at `/core/v1/`, `/transfer/v1/`, and `/swaps/v1/`.
Auth header: `X-Api-Key: <key>` or `Authorization: Bearer <key>`.

---

## Quick-start: zero to running a Lit Action

1. **Create an account** — `POST /core/v1/new_account` (or use the Dashboard). Save the returned `api_key` immediately; it is shown once.
2. **Add funds** — Dashboard "Add Funds" button or Stripe billing endpoints. Minimum $5.00.
3. **Create a wallet** — `GET /core/v1/create_wallet` with your API key. Returns a `wallet_address` (PKP).
4. **Create a group** — `POST /core/v1/add_group` to organize wallets and actions together.
5. **Register an action** — `POST /core/v1/add_action` with the IPFS CID of your JavaScript code.
6. **Wire them together** — `POST /core/v1/add_action_to_group` and `POST /core/v1/add_pkp_to_group`.
7. **Create a usage API key** — `POST /core/v1/add_usage_api_key` scoped to that group's `execute_in_groups`.
8. **Run it** — `POST /core/v1/lit_action` with the usage key, your code (or IPFS CID), and `js_params`.

After setup, daily use is just step 8: call the API with your usage key, action code, and parameters.

---

## Concepts

### Accounts
An account is the top-level entity. Creating one mints a master **account API key** and an initial wallet. The account key has full administrative control — never embed it in client code.

### Wallets (PKPs)
Programmable Key Pairs. Each is an elliptic-curve key pair held by the TEE. A wallet address is an Ethereum-compatible address. PKPs can sign data, hold funds, and be used by Lit Actions. Create more with `/create_wallet`.

### Groups
The primary organizational unit binding three things together:
- **Wallets (PKPs)** — which wallets can be used
- **IPFS Actions** — which Lit Actions can execute
- **Usage API Keys** — which keys have access (via permission arrays)

A usage API key can only run actions and use wallets that belong to groups it has been granted access to.

Use cases: single-dApp scoping, environment isolation (staging vs prod), safe key revocation.

### Actions (IPFS CIDs)
Immutable JavaScript programs stored on IPFS, identified by content hash (CID). Register them with `/add_action`, then assign to groups. The CID is keccak256-hashed on-chain for permission checks.

### Usage API Keys
Scoped, rotatable credentials for operational use. Created from the account key with granular permissions: which groups to execute in, whether to create/delete groups or PKPs. A compromised usage key only affects its permitted groups.

### Permission model
```
Account
  ├── Account API Key (full admin)
  ├── Usage API Keys (scoped per-group)
  ├── Groups
  │     ├── PKPs (wallets)
  │     └── IPFS Actions (permitted CIDs)
  └── Wallets (PKPs, can belong to multiple groups)
```

---

## Account management endpoints (`/core/v1/`)

All write operations cost $0.01. All read (GET list) operations are free.

### Account

| Endpoint | Method | Request body | Response | Notes |
|----------|--------|-------------|----------|-------|
| `/new_account` | POST | `{ "account_name": "", "account_description": "", "email": "" }` | `{ "api_key": "", "wallet_address": "" }` | `email` is optional (Stripe). **Key shown once.** |
| `/account_exists` | GET | — | `{ "exists": true }` | Uses API key from header. |

### Wallets (PKPs)

| Endpoint | Method | Request | Response | Notes |
|----------|--------|---------|----------|-------|
| `/create_wallet` | GET | — | `{ "wallet_address": "" }` | Mints a new PKP. Billable. |
| `/list_wallets` | GET | `?page_number=&page_size=` | `[{ "id": "", "name": "", "description": "", "wallet_address": "" }]` | Free. |
| `/list_wallets_in_group` | GET | `?group_id=&page_number=&page_size=` | Same shape | Free. |
| `/add_pkp_to_group` | POST | `{ "group_id": 1, "pkp_id": "" }` | `{ "success": true }` | Wallet can belong to multiple groups. |
| `/remove_pkp_from_group` | POST | `{ "group_id": 1, "pkp_id": "" }` | `{ "success": true }` | |

### Groups

| Endpoint | Method | Request body | Response | Notes |
|----------|--------|-------------|----------|-------|
| `/add_group` | POST | `{ "group_name": "", "group_description": "", "pkp_ids_permitted": [], "cid_hashes_permitted": [] }` | `{ "success": true, "group_id": "" }` | Pass `["0"]` in arrays for wildcard (all wallets/actions). Empty = none. |
| `/list_groups` | GET | `?page_number=&page_size=` | `[{ "group_id": "", "group_name": "", "group_description": "" }]` | Free. |
| `/update_group` | POST | `{ "group_id": 1, "name": "", "description": "", "pkp_ids_permitted": [], "cid_hashes_permitted": [] }` | `{ "success": true }` | |
| `/remove_group` | POST | `{ "group_id": "" }` | `{ "success": true }` | Affected usage keys lose access to that group. |

### Actions (IPFS)

| Endpoint | Method | Request body | Response | Notes |
|----------|--------|-------------|----------|-------|
| `/add_action` | POST | `{ "action_ipfs_cid": "Qm...", "name": "", "description": "" }` | `{ "success": true }` | CID is keccak256-hashed on server. Use raw CID here. |
| `/list_actions` | GET | `?group_id=&page_number=&page_size=` | `[{ "id": "", "name": "", "description": "" }]` | `group_id` is optional filter. Free. |
| `/add_action_to_group` | POST | `{ "group_id": 1, "action_ipfs_cid": "Qm..." }` | `{ "success": true }` | Use raw CID. |
| `/remove_action_from_group` | POST | `{ "group_id": 1, "hashed_cid": "0x..." }` | `{ "success": true }` | Use keccak256-hashed CID (0x-prefixed). |
| `/delete_action` | POST | `{ "hashed_cid": "0x..." }` | `{ "success": true }` | Use keccak256-hashed CID. |
| `/update_action_metadata` | POST | `{ "hashed_cid": "0x...", "name": "", "description": "" }` | `{ "success": true }` | |
| `/get_lit_action_ipfs_id` | POST | `{ "code": "..." }` | `{ "ipfs_id": "Qm..." }` | Get IPFS CID for inline code. |

**Important**: Creation endpoints (`add_action`, `add_action_to_group`) take raw IPFS CIDs. Modification/deletion endpoints (`delete_action`, `remove_action_from_group`, `update_action_metadata`) take keccak256-hashed CIDs (0x-prefixed hex).

### Usage API Keys

| Endpoint | Method | Request body | Response | Notes |
|----------|--------|-------------|----------|-------|
| `/add_usage_api_key` | POST | See below | `{ "success": true, "usage_api_key": "" }` | **Key shown once.** |
| `/list_api_keys` | GET | `?page_number=&page_size=` | Array of key objects | Free. |
| `/update_usage_api_key` | POST | Same fields as add + `usage_api_key` | `{ "success": true }` | |
| `/update_usage_api_key_metadata` | POST | `{ "usage_api_key": "", "name": "", "description": "" }` | `{ "success": true }` | |
| `/remove_usage_api_key` | POST | `{ "usage_api_key": "" }` | `{ "success": true }` | |

**Usage API key permission fields:**
```json
{
  "name": "my-app-key",
  "description": "Production key for price oracle",
  "can_create_groups": false,
  "can_delete_groups": false,
  "can_create_pkps": false,
  "manage_ipfs_ids_in_groups": [],
  "add_pkp_to_groups": [],
  "remove_pkp_from_groups": [],
  "execute_in_groups": [1, 2]
}
```
Use `[0]` as wildcard for "all groups" in any group array.

### Billing

| Endpoint | Method | Request | Response | Notes |
|----------|--------|---------|----------|-------|
| `/billing/balance` | GET | — | `{ "balance_cents": -500, "balance_display": "$5.00" }` | Negative = credits available. |
| `/billing/stripe_config` | GET | — | `{ "publishable_key": "" }` | No auth required. |
| `/billing/create_payment_intent` | POST | `{ "amount_cents": 500 }` | `{ "client_secret": "", "payment_intent_id": "" }` | Minimum 500 ($5.00). |
| `/billing/confirm_payment` | POST | `{ "payment_intent_id": "" }` | `{ "success": true }` | |

### Configuration

| Endpoint | Method | Response |
|----------|--------|----------|
| `/get_lit_action_client_config` | GET | Execution limits: `timeout_ms`, `memory_limit_mb`, `max_code_length`, `max_fetch_count`, etc. |
| `/get_node_chain_config` | GET | Supported chains: `chain_name`, `chain_id`, `is_evm`, `testnet`. |
| `/version` | GET | Server version, commit hash, submodule versions. |

---

## Running Lit Actions (`POST /core/v1/lit_action`)

### Request
```json
{
  "code": "const result = 'hello'; Lit.Actions.setResponse({ response: result });",
  "js_params": { "pkpId": "0x..." }
}
```

### Response
```json
{
  "response": "hello",
  "logs": "",
  "has_error": false
}
```

### Lit Actions SDK (available inside action code)

| Function | Parameters | Returns | Purpose |
|----------|-----------|---------|---------|
| `Lit.Actions.getPrivateKey` | `{ pkpId }` | Private key string | Get PKP's private key for signing |
| `Lit.Actions.Encrypt` | `{ pkpId, message }` | Ciphertext string | Encrypt data with PKP-derived key |
| `Lit.Actions.Decrypt` | `{ pkpId, ciphertext }` | Plaintext string | Decrypt data |
| `Lit.Actions.getLitActionPrivateKey` | — | Private key string | Get the executing action's own private key |
| `Lit.Actions.getLitActionPublicKey` | `{ ipfsId }` | Public key string | Get an action's public key |
| `Lit.Actions.getLitActionWalletAddress` | `{ ipfsId }` | Address string | Get an action's wallet address |
| `Lit.Actions.setResponse` | `{ response }` | void | Set the return value (non-strings are JSON-encoded) |

**Globals**: `ethers` (ethers.js v5), `Lit.Auth.authSigAddress`, `Lit.Auth.actionIpfsIdStack`.

### Resource limits

| Limit | Value |
|-------|-------|
| Max code size (inline) | 16 MB |
| Max `js_params` payload | 64 KB |
| Max execution time | 15 minutes |
| Max memory | 64 MB |
| Max HTTP fetches per action | 50 |
| Max response payload | 100 KB |
| Max console log output | 100 KB |
| Max key/signature operations | 10 |

### Example: Sign a message

```javascript
async function main() {
  const wallet = new ethers.Wallet(
    await Lit.Actions.getPrivateKey({ pkpId: params.pkpId })
  );
  const signature = await wallet.signMessage(params.message);
  Lit.Actions.setResponse({ response: JSON.stringify({ signature }) });
}
main();
```
Call with: `{ "code": "...", "js_params": { "pkpId": "0x...", "message": "hello" } }`

### Example: Encrypt and decrypt a secret

```javascript
// Encrypt
const ciphertext = await Lit.Actions.Encrypt({
  pkpId: params.pkpId,
  message: params.secret
});
Lit.Actions.setResponse({ response: JSON.stringify({ ciphertext }) });

// Decrypt (separate action call)
const plaintext = await Lit.Actions.Decrypt({
  pkpId: params.pkpId,
  ciphertext: params.ciphertext
});
Lit.Actions.setResponse({ response: JSON.stringify({ plaintext }) });
```

### Example: Fetch external data and sign it (oracle proof)

```javascript
async function main() {
  const res = await fetch("https://api.coingecko.com/api/v3/simple/price?ids=ethereum&vs_currencies=usd");
  const data = await res.json();
  const price = data.ethereum.usd;
  const payload = JSON.stringify({ price, timestamp: Date.now() });

  const wallet = new ethers.Wallet(
    await Lit.Actions.getPrivateKey({ pkpId: params.pkpId })
  );
  const signature = await wallet.signMessage(payload);
  Lit.Actions.setResponse({ response: JSON.stringify({ payload, signature }) });
}
main();
```

### Example: Read a smart contract

```javascript
async function main() {
  const provider = new ethers.providers.JsonRpcProvider(params.rpcUrl);
  const abi = ["function balanceOf(address) view returns (uint256)", "function symbol() view returns (string)"];
  const contract = new ethers.Contract(params.tokenAddress, abi, provider);

  const [balance, symbol] = await Promise.all([
    contract.balanceOf(params.walletAddress),
    contract.symbol()
  ]);

  Lit.Actions.setResponse({
    response: JSON.stringify({ balance: balance.toString(), symbol })
  });
}
main();
```

### Example: Send ETH from a PKP

```javascript
async function main() {
  const provider = new ethers.providers.JsonRpcProvider(params.rpcUrl);
  const wallet = new ethers.Wallet(
    await Lit.Actions.getPrivateKey({ pkpId: params.pkpId }),
    provider
  );
  const tx = await wallet.sendTransaction({
    to: params.to,
    value: ethers.utils.parseEther(params.amount)
  });
  const receipt = await tx.wait();
  Lit.Actions.setResponse({
    response: JSON.stringify({ txHash: receipt.transactionHash })
  });
}
main();
```

### Design patterns

1. **Gating logic** — Write access conditions as plain JavaScript (check balances, token holdings, timestamps) before signing or decrypting.
2. **Action-identity signing** — Use `getLitActionPrivateKey()` when the proof must bind to the immutable action code itself (verifiable via IPFS CID). Use `getPrivateKey({ pkpId })` when proof must bind to a specific wallet.
3. **PKP-as-data-vault** — Create a dedicated PKP per data boundary. Encrypt with it, gate decryption inside a Lit Action.
4. **Secure RPC URLs** — Encrypt API keys with a "secrets PKP", decrypt inside the TEE at runtime. Hard-code the base URL in action code for verifiability.

---

## Transfer API (`/transfer/v1/`)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/get_all_chains` | GET `?is_evm=&is_testnet=` | List supported chains. |
| `/get_api_key_balance` | GET `/<api_key>/<chain>` | Balance of API key's wallet. |
| `/get_pkp_balance` | GET `/<pkp_public_key>/<chain>` | Balance of a PKP on a chain. |
| `/get_address_balance` | GET `/<address>/<chain>` | Balance of any address. |
| `/send` | POST | Send funds from a PKP. Body: `api_key`, `pkp_public_key`, `chain`, `destination_address`, `amount`. |

---

## Swaps API (`/swaps/v1/`)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/new_quote_request` | POST | Create a swap request. |
| `/fill_quote` | POST | Provider fills the request. |
| `/accept_quote` | POST | Accept a quote. |
| `/get_swap_status` | GET `/<quote_id>` | Poll swap status. |
| `/get_quote_balances` | GET `/<quote_id>` | Check balances before swap. |
| `/attempt_swap_request` | GET `/<quote_id>` | Execute the swap. |
| `/get_open_swap_requests` | GET | List open requests. |
| `/get_open_quotes` | GET | List open quotes. |
| `/token_list` | GET | Supported tokens. |
| `/get_contract_address` | GET | Quote storage contract. |

**Flow**: `new_quote_request` → `fill_quote` → `accept_quote` → `get_quote_balances` → `attempt_swap_request`.

---

## Conventions

- **Request bodies**: `snake_case` (`api_key`, `group_id`, `action_ipfs_cid`).
- **JS SDK options**: `camelCase` (`apiKey`, `groupId`, `actionIpfsCid`); SDKs convert to snake_case.
- **Errors**: `{ "message": "...", "errors": [{ "description": "..." }] }`. HTTP 400 = bad input, 402 = insufficient credits, 403 = permission denied.
- **Pagination**: `page_number` (1-based) and `page_size` query params on all list endpoints.
- **CID formats**: Raw CID for creation, keccak256-hashed (0x-prefixed) for modification/deletion.

## Pricing

- Read operations (GET lists, config): **free**.
- Management writes (create/update/delete): **$0.01 per call**.
- Lit Action execution: **$0.01 per second**.
- Minimum credit purchase: **$5.00** (500 credits).
- No subscriptions, no expiration.

## JavaScript SDK

Use `core_sdk.js` in this folder:
```javascript
const client = new LitCoreClient({ baseUrl: "https://api.chipotle.litprotocol.com" });
const { apiKey, walletAddress } = await client.newAccount({
  accountName: "my-account",
  accountDescription: "demo"
});
```

## File references

- SDK: `lit-static/core_sdk.js`
- Endpoints: `lit-api-server/src/core/v1/endpoints/account_management.rs`, `actions.rs`, `billing.rs`, `configuration.rs`
- Request models: `lit-api-server/src/core/v1/models/request.rs`
- Response models: `lit-api-server/src/core/v1/models/response.rs`
- Guards/auth: `lit-api-server/src/core/v1/guards/apikey.rs`, `billing.rs`
- Lit Actions runtime: `lit-actions/ext/js/02_litActionsSDK.js`
- Lit Actions types: `lit-actions/packages/naga-la-types/types.d.ts`
- On-chain storage: `blockchain/lit_node_express/contracts/AccountConfigFacets/AppStorage.sol`
- OpenAPI spec: `spec.json` (auto-generated), Swagger UI at `/core/v1/swagger-ui/`
- Developer docs: `https://developer.litprotocol.com/`
