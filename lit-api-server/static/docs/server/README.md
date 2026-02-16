# Lit Node Simple API – SDK docs

Developer documentation for the JavaScript SDKs used with the Lit Node Simple API.

---

## Documentation index

| Page | Description |
|------|--------------|
| [Core SDK](core-sdk.md) | **core_sdk.js** — API key, handshake, mint PKP, sign with PKP, lit action, encrypt, decrypt, combine signature shares. |
| [PKP Sign](pkp-sign.md) | Signing messages with a PKP using the Core SDK (default scheme: EcdsaK256Sha256). |
| [Transfer SDK](transfer-sdk.md) | **transfer_sdk.js** — Get balance (API key, PKP, or address), send funds. |

---

## Style reference

These docs follow the same structure and tone as the [Lit Protocol developer docs](https://developer.litprotocol.com/sdk/auth-context-consumption/pkp-sign): short intro, Prerequisites, Quick Example, then reference tables and notes.

---

## SDK files

- **static/core_sdk.js** — Core v1 endpoints (see [core-sdk.md](core-sdk.md)).
- **static/transfer_sdk.js** — Transfer endpoints (see [transfer-sdk.md](transfer-sdk.md)).

Base URL default for both: `http://localhost:8000`.
