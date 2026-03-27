# PKP Sign

> Use your PKP to sign a message with the Lit Node Simple API. The core SDK uses **EcdsaK256Sha256** (secp256k1 + SHA-256) by default, matching common Ethereum-style signing.

**Tip:** For the full Lit Protocol SDK (multiple chains and schemes), see [PKP Sign](https://developer.litprotocol.com/sdk/auth-context-consumption/pkp-sign) in the Lit developer docs. This page documents the **Simple API** flow via `core_sdk.js`.

---

## Prerequisites

- **API key** — From `getApiKey()` (hex-encoded wallet secret).
- **PKP public key** — From `mintPkp(apiKey)` (the PKP to sign with).

---

## Quick Example

```js
import { createClient, SIGNING_SCHEME_ECDSA_K256_SHA256 } from './core_sdk.js';

const client = createClient('http://localhost:8000');

// Message is UTF-8 encoded; hashing (e.g. Keccak256 for Ethereum) is handled by the API.
const messageToSign = 'hello world';

const { shares, curve_type } = await client.signWithPkp({
  apiKey: api_key,
  pkpPublicKey: pkp_public_key,
  message: messageToSign,
  // signingScheme is optional; defaults to EcdsaK256Sha256
});

// Combine threshold shares into a single signature
const { signature, r, s, v, recovery_id } = await client.combineSignatureShares({
  apiKey: api_key,
  shares,
});
```

---

## Signing scheme

The default and only scheme documented here for the Simple API is:

| Scheme | Curve | Use case |
|--------|--------|----------|
| `EcdsaK256Sha256` | secp256k1 | Ethereum-style signing; message is hashed (e.g. Keccak256) by the server before signing. |

**Note:** Pass `signingScheme: SIGNING_SCHEME_ECDSA_K256_SHA256` (or `'EcdsaK256Sha256'`) if you set it explicitly. Other schemes may be supported by the backend; see `core/v1` and the Rust API for details.

---

## Flow summary

1. **getApiKey()** — Get an API key (and funded wallet).
2. **mintPkp(apiKey)** — Mint a PKP for that wallet.
3. **signWithPkp({ apiKey, pkpPublicKey, message, signingScheme? })** — Request signature shares from the network; returns `shares` and `curve_type`.
4. **combineSignatureShares({ apiKey, shares })** — Combine `shares` into a single `signature` plus `r`, `s`, `v`, `recovery_id`.

The `shares` returned by `signWithPkp` are full signing response objects (not raw strings). Pass them as-is to `combineSignatureShares`.
