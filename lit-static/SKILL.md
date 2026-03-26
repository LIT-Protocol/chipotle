---
name: lit-peer-api-endpoints
description: Teaches how to use the Lit Node Simple API endpoints and for which purposes. Use when an AI needs to call the API, choose the right endpoint, or implement flows (auth, signing, encrypt/decrypt, transfers, swaps). Covers /core/v1/, /transfer/v1/, /swaps/v1/.
---

# Lit Node Simple API – Endpoint usage for AIs

Base URL is typically `http://localhost:8000`. Routes are mounted at `/core/v1/`, `/transfer/v1/`, and `/swaps/v1/`. Use the JavaScript SDKs in this folder (`core_sdk.js`, `transfer_sdk.js`, `swaps_sdk.js`) or call the HTTP endpoints directly.

---

## When to use which API

| Need | Use |
|------|-----|
| **Identity / wallet for this session** | Core: `get_api_key` → then use `api_key` for all wallet-scoped core and transfer calls. |
| **Prove control of a key (PKP)** | Core: `mint_pkp` (get a PKP), then `sign_with_pkp` + `combine_signature_shares` to produce a single signature. |
| **Run custom logic on Lit nodes** | Core: `lit_action` (JavaScript code + optional `js_params`). |
| **Encrypt so only a wallet can decrypt** | Core: `encrypt` (time-lock for wallet) → later `decrypt` with same `api_key` and `ciphertext` + `data_to_encrypt_hash`. |
| **Check or spend balances on chains** | Transfer: `get_all_chains`, then `get_api_key_balance` / `get_pkp_balance` / `get_address_balance`; to send funds use `send`. |
| **Swap / quote flows** | Swaps: create request → fill quote → accept quote → check balances → attempt swap. |

---

## Core API (`/core/v1/`) – SDK: `core_sdk.js`

**Purpose**: Lit node primitives: session identity, PKP mint/sign, Lit actions, time-lock encrypt/decrypt, ledger balance.

| Endpoint | Method | Purpose | When to use |
|----------|--------|---------|-------------|
| **get_api_key** | GET | Create a new API key (wallet secret) and fund it. | Start of any flow that needs a wallet identity. Use returned `api_key` for mint_pkp, sign, encrypt/decrypt, lit_action, combine_signature_shares, and transfer. |
| **handshake** | GET | Handshake with validators; returns their responses. | When you need to establish or verify validator connectivity. |
| **mint_pkp** | GET `/<api_key>` | Mint a PKP for the wallet tied to `api_key`. | When you need a programmable key pair (e.g. to sign on behalf of a user or run Lit actions with a PKP). |
| **sign_with_pkp** | POST JSON | Sign a message with a PKP. Returns **SignWithPkpResponse** (signing_scheme, signed_digest, public_key, share_type, shares, etc.). | To produce a threshold signature; pass the full response to **combine_signature_shares** as share_data. |
| **combine_signature_shares** | POST JSON | Turn a **SignWithPkpResponse** (from sign_with_pkp) into one signature. Request body: `api_key` + `share_date` (the full sign response). | After every sign_with_pkp when you need the final signature (e.g. for broadcasting a tx). |
| **lit_action** | POST JSON | Run JavaScript Lit action (code + optional js_params). | Custom logic on Lit nodes (e.g. signing, condition checks, multi-step flows). |
| **encrypt** | POST JSON | Time-lock encrypt a message for the wallet identified by `api_key`. | When only that wallet (or someone with its key) should decrypt later. |
| **decrypt** | POST JSON | Decrypt ciphertext; server fetches decryption shares. Body: `api_key`, `ciphertext`, `data_to_encrypt_hash` (from encrypt). | After encrypt; use same `api_key` and the encrypt response fields. No client-side shares. |
| **get_ledger_balance** | GET `/<api_key>` | Ledger (inquiry) balance for the API key’s wallet. | To check on-ledger balance for that wallet. |

**Typical core flow**: `get_api_key` → `mint_pkp` → `sign_with_pkp` → `combine_signature_shares` (and/or `encrypt` → `decrypt`, and/or `lit_action`).

---

## Transfer API (`/transfer/v1/`) – SDK: `transfer_sdk.js`

**Purpose**: Chain metadata, balance lookups (by API key, PKP, or address), and sending funds (PKP-signed).

| Endpoint | Method | Purpose | When to use |
|----------|--------|---------|-------------|
| **get_all_chains** | GET `?is_evm=&is_testnet=` | List supported chains (EVM, non-EVM, testnet). | To get valid `chain` values for balance/send and for UI (display_name, token). |
| **get_api_key_balance** | GET `/<api_key>/<chain>` | Balance of the wallet for that API key on the given chain. | Check wallet balance before/after transfers. |
| **get_pkp_balance** | GET `/<pkp_public_key>/<chain>` | Balance of a PKP address on a chain. | Check PKP balance (e.g. before swap or send). |
| **get_address_balance** | GET `/<address>/<chain>` | Balance of an arbitrary address on a chain. | Generic balance lookup. |
| **send** | POST JSON | Send funds from a PKP to a destination (PKP-signed tx). | To move funds: need `api_key`, `pkp_public_key`, `chain`, `destination_address`, `amount`. |

**When to use**: Any balance check or send on a supported chain; chain ids come from `get_all_chains`. Use the same `api_key` from core for consistency.

---

## Swaps API (`/swaps/v1/`) – SDK: `swaps_sdk.js`

**Purpose**: Swap intents: create swap requests, get quotes, check quote balances, execute swaps.

| Endpoint | Method | Purpose | When to use |
|----------|--------|---------|-------------|
| **get_contract_address** | GET | Quote storage contract address. | If the client needs the contract address for display or direct calls. |
| **token_list** | GET | Supported tokens list. | To show token options in a swap UI. |
| **new_quote_request** | POST JSON | Create a swap request (origin/destination chain, amounts, addresses). Returns `swap_request_id`, `transaction_hash`, etc. | Start a swap: user wants to swap X on chain A for Y on chain B. |
| **fill_quote** | POST JSON | Provider fills the request; returns `quote_id`, `transaction_hash`, etc. | After a swap request exists; solver/provider fills it. |
| **accept_quote** | POST JSON | Accept a quote; returns PKP address to send funds to. | When user/solver accepts the quoted terms. |
| **get_swap_status** | GET `/<quote_id>` | Status of a swap by quote id. | Poll or display status (Pending, Processing, Success, etc.). |
| **get_open_swap_requests** | GET | Open swap requests from contract. | Solver: see what requests need quotes. |
| **get_open_quotes** | GET | Open quotes from contract. | Solver: see available quotes. |
| **get_quote_balances** | GET `/<quote_id>` | PKP balances on source/destination chains for that quote. | Before attempt_swap_request: ensure sufficient balance. |
| **attempt_swap_request** | GET `/<quote_id>` | Execute the swap (origin + destination transfers). | After balances are sufficient; completes the swap flow. May return 402 if insufficient funds. |

**Typical swap flow**: User: `new_quote_request` → (solver fills) `fill_quote` → `accept_quote` → `get_quote_balances` → `attempt_swap_request`. Solver: `get_open_swap_requests` / `get_open_quotes` → `fill_quote` (and related).

---

## Conventions for implementers

- **Request bodies**: snake_case (`api_key`, `data_to_encrypt_hash`, `pkp_public_key`). Match Rust request structs in `src/core/v1/models/request.rs` and transfer/swaps models.
- **SDK options**: camelCase in JS (`apiKey`, `dataToEncryptHash`, `pkpPublicKey`); SDKs convert to snake_case when building JSON.
- **Errors**: Responses include a body with a message (e.g. `{ "error": "..." }`). SDKs parse this and throw with that message; use it when advising users or retrying.
- **Decrypt**: Only `api_key`, `ciphertext`, `data_to_encrypt_hash` (from encrypt response). No shares in the request.
- **Combine signature shares**: Request body is `api_key` + `share_date` (the full **SignWithPkpResponse** from sign_with_pkp), not an array of shares. SDK: `combineSignatureShares({ apiKey, shareData })` where `shareData` is the full object returned by `signWithPkp`.

## File references

- Core: `src/core/v1/endpoints.rs`, `src/core/v1/models/request.rs`, `src/core/v1/models/response.rs`
- Transfer: `src/abstractions/transfer/endpoints.rs`, `src/abstractions/transfer/models.rs`
- Swaps: `src/abstractions/intents/swaps/endpoints.rs`, `src/abstractions/intents/swaps/models.rs`
