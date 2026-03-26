# JavaScript Transfer SDK

> Wrapper for transfer endpoints: balance lookups (API key wallet, PKP, or arbitrary address) and sending funds. Use `transfer_sdk.js` when the Lit Node Simple API has transfer routes mounted.

**Tip:** Balance and send operations depend on the configured chains (e.g. Ethereum, Solana). See the API’s transfer/chain configuration for supported `chain` values.

---

## Prerequisites

- **Base URL** — Your Lit Node Simple API server (e.g. `http://localhost:8000`).
- **Transfer routes** — Server must mount transfer routes (`get_api_key_balance`, `get_pkp_balance`, `get_address_balance`, `send`).
- **API key** — From core SDK `getApiKey()` when using wallet or send flows.
- **PKP public key** — From core SDK `mintPkp(apiKey)` when querying PKP balance or sending from a PKP.

---

## Quick Example

```js
import { createTransferClient } from './transfer_sdk.js';

const client = createTransferClient('http://localhost:8000');

// Balance for the wallet derived from an API key
const walletBalance = await client.getApiKeyBalance(apiKey, 'Ethereum');

// Balance for a PKP address
const pkpBalance = await client.getPkpBalance(pkp_public_key, 'Ethereum');

// Balance for any address (e.g. 0x...)
const addressBalance = await client.getAddressBalance('0x742d35Cc6634C0532925a3b844Bc454e4438f44e', 'Ethereum');

// Send funds (PKP-signed)
const result = await client.send({
  apiKey,
  pkpPublicKey,
  chain: 'Ethereum',
  destinationAddress: '0x...',
  amount: '1000000000000000000',
});
```

---

## Client & factory

| Method | Description |
|--------|-------------|
| `createTransferClient(baseUrl?)` | Returns a `LitTransferApiClient`. Default base URL: `http://localhost:8000`. |
| `new LitTransferApiClient({ baseUrl })` | Construct transfer client with optional `baseUrl`. |

---

## Balance endpoints

All balance methods return a **GetBalanceResponse**: `{ address, balance, chain, symbol }`.

| Method | Endpoint | Description |
|--------|----------|-------------|
| `getApiKeyBalance(apiKey, chain)` | `GET /get_api_key_balance/<api_key>/<chain>` | Balance for the wallet identified by the API key. |
| `getPkpBalance(pkpPublicKey, chain)` | `GET /get_pkp_balance/<pkp_public_key>/<chain>` | Balance for the PKP’s on-chain address. |
| `getAddressBalance(address, chain)` | `GET /get_address_balance/<address>/<chain>` | Balance for an arbitrary address (e.g. `0x...` for EVM). |

**Note:** `chain` is a string identifier (e.g. `"Ethereum"`, `"Solana"`). Supported values depend on server configuration.

---

## Send

| Method | Endpoint | Returns |
|--------|----------|---------|
| `send({ apiKey, pkpPublicKey, chain, destinationAddress, amount })` | `POST /send` | **TransferResponse** — `txn_id`, `success`, `chain`, `origin_symbol`, `origin_amount`, `gas`, `timestamp`, `destination_address`. |

Send is PKP-signed: the transaction is authorized by the PKP associated with `pkpPublicKey`, using the wallet from `apiKey` for session/signing flow.

---

## Response types (summary)

- **GetBalanceResponse** — `address`, `balance`, `chain`, `symbol` (same for all three balance methods).
- **TransferResponse** — `txn_id`, `success`, `chain`, `origin_symbol`, `origin_amount`, `gas`, `timestamp`, `destination_address`.

All methods return Promises and throw on non-2xx HTTP responses.
