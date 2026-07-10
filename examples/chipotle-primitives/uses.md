# Primitive reference — what each function does and when to use it

Every function below lives in one of the `lit-action-*` modules and is also
re-exported from `lit-action-primitives.js`. Gates and asserts **fail closed**
(they throw `GateError`); your `main` catches and returns
`{ authorized: false, reason }`. Reads and signers return values.

Reminder: anything that defines the trust boundary (host regexes, min-source
counts, spread caps, allowed addresses, base URLs) is passed in as an argument
that **the action source hardcodes** — never forwarded from `js_params`.

---

## `lit-action-core.js` — hashing, ABI, denial base

### `keccak256(bytes) -> 0xhex`
keccak-256 of raw bytes (`@noble/hashes`), returned 0x-prefixed.
**Use when** you need a raw hash — e.g. hashing packed bytes you already built,
or computing an event-topic to match in `readAndValidateLog`.

### `keccak256Utf8(text) -> 0xhex`
keccak-256 of a UTF-8 string.
**Use when** binding an identifier to text so it can't be swapped — e.g.
`questionId = keccak256Utf8(questionText)` in an oracle, or an event signature
hash like `keccak256Utf8("Transfer(address,address,uint256)")`.

### `arrayify(0xhex) -> Uint8Array`
0x-hex to bytes (ethers `arrayify` equivalent).
**Use when** you have a digest as hex and need the byte form to feed a signer.

### `abiEncode(types, values) -> Uint8Array`
ABI-encodes a tuple exactly like `abi.encode(...)` on-chain (audited coder,
selector stripped).
**Use when** building the preimage a contract will re-derive — almost always via
`buildAuthDigest`/`signDigest` rather than directly. Pass ints as
number/bigint/string, address/bytes as 0x-hex (see README value-type table).

### `GateError` / `deny(reason)`
The throw-to-deny convention.
**Use when** writing your own gate: call `deny("why")` to fail closed; catch
`GateError` (or any `Error`) in `main`.

---

## `lit-action-rpc.js` — RPC trust anchors & chain safety

### `rpc(url, method, params) -> result`
Hardened JSON-RPC POST with `redirect: "error"` (closes the
open-redirect-after-pin hole) that throws on HTTP/RPC error.
**Use when** making any raw JSON-RPC call. Prefer this over a bare `fetch` so a
pinned host that 30x-redirects to an attacker is rejected.

### `assertAllowedRpc({ chainId, rpcUrl, policy }) -> chainPolicy`
Requires https + a hostname matching the per-chain `policy`, and returns that
chain's policy (e.g. `{ hostRegex, minConfirmations }`).
**Use when** an action accepts an `rpcUrl` from the caller. Run this first — it
pins the node so subsequent reads are trustworthy. `policy` is your hardcoded
map.

### `assertChainId({ rpcUrl, expectedChainId }) -> number`
Cross-checks `eth_chainId` against what you expect.
**Use when** you've pinned the host and want defense-in-depth that the node is
actually serving the chain you think (e.g. before signing a chain-scoped
authorization).

### `requireConfirmations({ rpcUrl, blockNumber, minConf }) -> number`
Reorg defense — fails unless the source block is buried `minConf` deep.
**Use when** signing off an on-chain event (a deposit, a fill, a settlement):
don't authorize until the triggering tx can't be reorged out.

---

## `lit-action-onchain.js` — reads & writes

### `readContract({ rpcUrl, address, abi, method, args }) -> decoded`
Generic `eth_call` view helper; decodes via JSON ABI fragments.
**Use when** reading any contract view function — balances, config flags, order
structs. `args` is a named object or positional array.

### `readAndValidateLog({ rpcUrl, txHash, logIndex, expectedContract, expectedTopic }) -> log`
Fetches a receipt, confirms it succeeded, and confirms the log at `logIndex`
came from the expected contract and event signature.
**Use when** an action reacts to a specific on-chain event and must prove that
event really happened from the right source before acting (e.g. cross-chain
"mint on B because burn happened on A").

### `sendEth({ pkpId, to, amountEth, chainId, rpcUrl }) -> { txHash, ... }`
Builds/signs/broadcasts a native transfer from a PKP wallet (EIP-1559, fees from
`eth_gasPrice`).
**Use when** the action itself must move native funds — e.g. a funded PKP
relayer paying out, refunding gas, or sweeping.

### `bindToOnchainOrder({ rpcUrl, settlementContract, depositId, requested, orderAbi }) -> order`
Reconstructs an order from a pinned settlement contract and requires the
caller's recipient/token/amount to match what the order actually says.
**Use when** a bot/solver asks you to authorize a payout. This is the kill shot
for exfiltration — the bot can claim any recipient in `js_params`, but the order
on-chain says who really gets paid.

---

## `lit-action-gating.js` — access control (fail closed)

### `gateEthBalance({ address, minWei, rpcUrl })`
Denies unless native balance ≥ `minWei`.
**Use when** token-gating on ETH holdings (e.g. "must hold ≥ 0.1 ETH to use this
action").

### `gateErc20Balance({ holder, token, minAmount, rpcUrl })`
Denies unless ERC-20 `balanceOf(holder)` ≥ `minAmount`.
**Use when** gating on a fungible token balance — membership tiers, staking
minimums, fee-token checks.

### `gateNftOwnership({ holder, nftContract, rpcUrl, tokenId })`
Denies unless the holder owns the NFT (ERC-721 `balanceOf > 0`, or ERC-1155
`balanceOf(holder, tokenId) > 0` when `tokenId` is given).
**Use when** gating behind an NFT membership/pass.

### `gateTimeWindow({ startUnix, endUnix })`
Rejects outside the inclusive time window.
**Use when** an action should only run during a sale, an auction, a claim
period, or after a vesting cliff.

### `gateSanctions({ address, screeningRpcUrl })`
Staticcalls the hardcoded Chainalysis oracle on a pinned mainnet RPC; denies if
the address is sanctioned.
**Use when** compliance requires screening a counterparty before signing a
transfer or payout.

### `gateAuthToken({ token, verifyUrl })`
POSTs the token to your auth service and requires `{ valid: true }`.
**Use when** access depends on an off-chain system (a logged-in session, a
subscription, an API entitlement).

### `requireCallerSignature({ message, signature, allowedAddress }) -> recovered`
Recovers the EIP-191 signer and requires it to equal `allowedAddress`.
**Use when** only a specific owner may trigger an action. The `message` must
carry a nonce + deadline (see `newNonce` / `withDeadline`) for replay safety.

### `requirePkpAllowed({ pkpAddress, allowedPkp })`
Case-insensitive allowlist check for a PKP address.
**Use when** restricting an action to one or more known PKPs (the gateway has
already proven PKP ownership; this just enforces the allowlist).

### `combineGates([...gateFns]) -> results[]`
Runs gate thunks in order, failing closed on the first denial.
**Use when** an action needs several conditions AND-ed together:
`await combineGates([() => gateSanctions(...), () => gateErc20Balance(...)])`.
For OR logic, try/catch individual gates.

---

## `lit-action-signing.js` — signing & action identity

### `signWithPkp({ pkpId, message }) -> { signature, signer }`
EIP-191 personal-sign with a PKP wallet key (account/user identity).
**Use when** the signature should be attributable to a user's PKP wallet rather
than to the action code.

### `signWithAction({ message }) -> { signature, signer }`
EIP-191 personal-sign with the action's CID-derived key.
**Use when** the signature must prove "this exact code approved this." A contract
pins the action's derived address; editing the action changes the CID and the
signer, so the approval can't be forged by modified code.

### `buildAuthDigest({ types, values }) -> { digest, digestBytes }`
`abi.encode -> keccak256 -> arrayify`, the exact tuple your verifying contract
re-derives before `ecrecover`.
**Use when** you want the digest without signing yet (e.g. to log it, or to sign
with a custom key path).

### `signDigest({ types, values, useAction, pkpId }) -> { signature, signer, digest }`
`buildAuthDigest` + sign in one call — the core of every authorization
primitive.
**Use when** producing the signature a contract will verify: encode the same
tuple the contract checks (`token, recipient, amount, nonce, deadline, vault,
chainId`, etc.) and sign it. Default signs with the action key; set
`useAction: false` + `pkpId` for a PKP.

### `recoverSigner({ message, signature }) -> address`
Recovers the EIP-191 signer address.
**Use when** verifying a signature you received and you want the address back
(e.g. to look it up), rather than just asserting equality.

### `getActionAddress({ ipfsId }) -> address`
Derives a sibling action's wallet address from its IPFS CID.
**Use when** wiring two actions together — e.g. pinning a price-oracle action's
address into a consumer contract's constructor.

---

## `lit-action-secrets.js` — PKP-as-vault

### `sealToVault({ pkpId, plaintext }) -> ciphertext`
Encrypts plaintext to a PKP; store the ciphertext anywhere.
**Use when** provisioning a secret (an API key, a webhook URL) that only this
PKP context can later decrypt.

### `openFromVault({ pkpId, ciphertext }) -> plaintext`
Decrypts ciphertext sealed to a PKP.
**Use when** you need the secret and have already passed your access gates.

### `withSecret({ pkpId, ciphertext }, async (secret) => {...}) -> result`
Decrypt-use-discard: the plaintext only exists for the callback's lifetime.
**Use when** using a secret for a single operation (one signed API request) and
you want to minimize how long it sits in memory.

### `assembleSecretRpcUrl({ pkpId, encryptedKey, baseUrl }) -> url`
Decrypts the key portion in-TEE and appends it to a hardcoded `baseUrl`.
**Use when** an RPC/provider URL embeds a secret API key but the host/chain must
stay verifiable — keep `baseUrl` in source, encrypt only the key.

---

## `lit-action-oracles.js` — consensus & oracles

### `requireStrictAgreement({ sources, fetchSource, minSources }) -> value`
Fetches each source, requires ≥ `minSources` successes, and denies unless they
agree byte-for-byte.
**Use when** the fact is discrete and must be unanimous — "is this address
sanctioned?", "did this event occur?". Disagreement should block, not average.

### `medianWithSpreadCheck({ observations, minSources, maxSpreadBps }) -> median`
Median of the observations; denies if `(max-min)/median` exceeds the cap. BigInt
throughout.
**Use when** aggregating continuous values (prices, rates) and you want a manipulation
guard — a single rogue feed can't drag the result if the spread blows the cap.

### `signedPriceFeed({ asset, sources, decimals, registry, deadline }) -> { price, signature, ... }`
Fetches N prices, median-with-spread-check, BigInt fixed-point scaling, then
signs the result.
**Use when** publishing an attestable price on-chain — the consumer contract
verifies the signature against the action's pinned address.

### `aiConsensus({ question, providers, minAgreement }) -> { questionId, answer, agreed }`
Polls LLM providers for a categorical YES/NO/UNCLEAR and requires `minAgreement`
to concur; binds `questionId = keccak256(question)`.
**Use when** resolving a subjective/real-world question (a prediction market,
content moderation) where you want multiple models to agree before acting.

---

## `lit-action-replay.js` — replay safety

### `newNonce() -> 0xhex`
Random 32-byte nonce (Web Crypto).
**Use when** issuing an authorization — fold the nonce into the signed digest so
the same signature can't be replayed (the contract tracks used nonces).

### `withDeadline(seconds) -> unixSeconds`
A deadline `seconds` in the future.
**Use when** issuing an authorization that should expire — fold it into the
digest and have the contract reject past it.

### `assertNotExpired({ deadline })`
Rejects a stale authorization before signing.
**Use when** an action re-signs or acts on a caller-supplied deadline — bail
early if it's already past instead of producing a useless signature.
