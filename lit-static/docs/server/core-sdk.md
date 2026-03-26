# JavaScript Core SDK

> Wrapper for the Lit Node Simple API v1 core endpoints. Use `core_sdk.js` for API key management, PKP minting, signing, lit actions, encryption, and signature combination.

**Tip:** See the [PKP Sign](pkp-sign.md) page for signing with your PKP using the default scheme, and the API reference in `core_sdk.js` for full types.

---

## Prerequisites

- **Base URL** — Your Lit Node Simple API server (e.g. `http://localhost:8000`).
- **API key** — From `getApiKey()` for wallet-scoped operations (mint PKP, sign, encrypt, lit action).

---

## Quick Example

```js
import { createClient, SIGNING_SCHEME_ECDSA_K256_SHA256 } from './core_sdk.js';

const client = createClient('http://localhost:8000');

// 1. Get API key (creates & funds a wallet)
const { api_key, wallet_address } = await client.getApiKey();

// 2. Mint a PKP for this wallet
const { pkp_public_key } = await client.mintPkp(api_key);

// 3. Sign a message with the PKP (EcdsaK256Sha256 by default)
const { shares, curve_type } = await client.signWithPkp({
  apiKey: api_key,
  pkpPublicKey: pkp_public_key,
  message: 'hello world',
});

// 4. Combine shares into a single signature
const { signature, r, s, v, recovery_id } = await client.combineSignatureShares({
  apiKey: api_key,
  shares,
});
```

---

## Client & factory

| Method | Description |
|--------|-------------|
| `createClient(baseUrl?)` | Returns a `LitNodeSimpleApiClient`. Default base URL: `http://localhost:8000`. |
| `new LitNodeSimpleApiClient({ baseUrl })` | Construct client with optional `baseUrl`. |

---

## API key & handshake

| Method | Endpoint | Returns |
|--------|----------|---------|
| `getApiKey()` | `GET /get_api_key` | `{ api_key, wallet_address }` — New hex API key and wallet address (wallet is funded). |
| `handshake()` | `GET /handshake` | `{ responses }` — Validator handshake responses. |

---

## PKP: mint & sign

| Method | Endpoint | Returns |
|--------|----------|---------|
| `mintPkp(apiKey)` | `GET /mint_pkp/<api_key>` | `{ pkp_public_key }` |
| `signWithPkp({ apiKey, pkpPublicKey, message, signingScheme? })` | `POST /sign_with_pkp` | `{ shares, curve_type }` — Pass `shares` to `combineSignatureShares`. |
| `combineSignatureShares({ apiKey, shares })` | `POST /combine_signature_shares` | `{ signature, signed_data, verifying_key, r, s, v, recovery_id }` |

**Note:** `signingScheme` defaults to `EcdsaK256Sha256` (secp256k1 + SHA-256). Use the exported constant `SIGNING_SCHEME_ECDSA_K256_SHA256` when you need to pass it explicitly.

---

## Lit action

| Method | Endpoint | Returns |
|--------|----------|---------|
| `litAction({ apiKey, code, jsParams? })` | `POST /lit_action` | `{ responses }` — Array of `{ signatures, response, logs }` per execution. |

`code` is the lit action JavaScript source. Optional `jsParams` is a JSON object passed into the action.

---

## Encrypt & decrypt

| Method | Endpoint | Returns |
|--------|----------|---------|
| `encrypt({ apiKey, message })` | `POST /encrypt` | `{ ciphertext, data_to_encrypt_hash }` — Store both for decrypt. |
| `decrypt({ apiKey, shares, ciphertext, dataToEncryptHash })` | `POST /decrypt` | `{ decrypted_text }` — `shares` come from the network/nodes. |

Time-lock encryption is scoped to the wallet for the given `api_key`. For decrypt you need decryption shares from the Lit network and the same `data_to_encrypt_hash` returned by `encrypt`.

---

## Response types (summary)

- **GetApiKeyResponse** — `api_key`, `wallet_address`
- **MintPkpResponse** — `pkp_public_key`
- **SignWithPkpResponse** — `shares` (array of signing response objects), `curve_type`
- **CombineSignatureSharesResponse** — `signature`, `signed_data`, `verifying_key`, `r`, `s`, `v`, `recovery_id`
- **EncryptResponse** — `ciphertext`, `data_to_encrypt_hash`
- **DecryptResponse** — `decrypted_text`
- **LitActionResponses** — `responses` (array of `{ signatures, response, logs }`)

All methods return Promises and throw on non-2xx HTTP responses.
