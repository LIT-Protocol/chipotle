# chipotle-primitives

A catalog of reusable Lit Action primitives, implemented as composable helpers
on top of **audited libraries** rather than hand-rolled crypto:

- [`micro-eth-signer@0.19.0`](https://github.com/paulmillr/micro-eth-signer) (Paul Miller) — `addr`, `eip191Signer`, `Transaction`, the ABI coder, wei math
- [`@noble/hashes@1.4.0`](https://github.com/paulmillr/noble-hashes) (Paul Miller) — keccak-256, hex/byte utilities

Everything here is glue around those plus the `Lit.Actions.*` host API and the
Deno globals `fetch` / `crypto`. No bespoke secp256k1, no bespoke ABI encoder.

## Files (grouped by concern)

| Module | Primitives |
| --- | --- |
| `lit-action-core.js` | `keccak256`, `keccak256Utf8`, `arrayify`, `abiEncode`, `GateError`, `deny` (shared base; imports the npm libs) |
| `lit-action-rpc.js` | `rpc`, `assertAllowedRpc`, `assertChainId`, `requireConfirmations` |
| `lit-action-onchain.js` | `readContract`, `readAndValidateLog`, `sendEth`, `bindToOnchainOrder` |
| `lit-action-gating.js` | `gateEthBalance`, `gateErc20Balance`, `gateNftOwnership`, `gateTimeWindow`, `gateSanctions`, `gateAuthToken`, `requireCallerSignature`, `requirePkpAllowed`, `combineGates` |
| `lit-action-signing.js` | `signWithPkp`, `signWithAction`, `buildAuthDigest`, `signDigest`, `recoverSigner`, `getActionAddress` |
| `lit-action-secrets.js` | `sealToVault`, `openFromVault`, `withSecret`, `assembleSecretRpcUrl` |
| `lit-action-oracles.js` | `requireStrictAgreement`, `medianWithSpreadCheck`, `signedPriceFeed`, `aiConsensus` |
| `lit-action-replay.js` | `newNonce`, `withDeadline`, `assertNotExpired` |
| `lit-action-primitives.js` | barrel — re-exports all of the above |

See [`uses.md`](./uses.md) for a per-function description and a "when to use it"
example for every primitive.

## The one rule

Anything that defines the **trust boundary** — host whitelists, min-source
counts, spread caps, allowed addresses, base RPC URLs, oracle addresses — must
be **hardcoded in the action source**, never read from `js_params`. The
whitelist is part of the action code, so changing it changes the IPFS CID and
therefore the action-derived signer address. That is the property that makes the
gate trustworthy. Caller-supplied policy is theater.

These primitives take thresholds as **arguments**; the action that imports them
passes hardcoded constants. Do not forward a `policy` object straight out of
`js_params` into them.

## Denials fail closed

Gates **throw** `GateError` on denial; they never return a soft "denied" object.
Your `main` catches and translates:

```js
import * as P from "./lit-action-primitives.js";

// Trust boundary — hardcoded in THIS file. Editing it changes the CID + signer.
const POLICY = {
  rpc: {
    84532: { hostRegex: /^base-sepolia\.g\.alchemy\.com$/i, minConfirmations: 2 },
  },
};
const OWNER = "0xYourBoundOwner";

async function main({ chainId, rpcUrl, token, to, amount, nonce, deadline, signature }) {
  try {
    P.assertAllowedRpc({ chainId, rpcUrl, policy: POLICY.rpc });
    await P.assertChainId({ rpcUrl, expectedChainId: chainId });
    P.assertNotExpired({ deadline });

    // Caller must sign a message carrying nonce + deadline (replay safety).
    const message = `withdraw\n${token}\n${to}\n${amount}\n${nonce}\n${deadline}`;
    P.requireCallerSignature({ message, signature, allowedAddress: OWNER });

    // Sign the exact tuple the verifying contract re-derives with ecrecover.
    const { signature: authSig, signer, digest } = await P.signDigest({
      types: ["address", "address", "uint256", "bytes32", "uint256", "uint256"],
      values: [token, to, amount, nonce, deadline, chainId],
      useAction: true,
    });

    return Lit.Actions.setResponse({
      response: JSON.stringify({ authorized: true, authSig, signer, digest }),
    });
  } catch (e) {
    return Lit.Actions.setResponse({
      response: JSON.stringify({ authorized: false, reason: e.message }),
    });
  }
}
```

## Value-type conventions for `abiEncode` / `signDigest`

`abiEncode(types, values)` feeds micro-eth-signer's ABI coder, which (via
micro-packed) is stricter than ethers about JS types. The wrapper coerces the
common cases for you:

- `uint*` / `int*` — pass `number`, `bigint`, or decimal `string`; coerced with `BigInt`.
- `address` — pass a `0x`-hex string.
- `bytes` / `bytesN` — pass a `0x`-hex string (converted to bytes) or a `Uint8Array`.
- `bool` — pass a boolean.
- `string` — pass a string.

The output bytes are the bare head/tail tuple encoding (selector stripped), so
`keccak256(abiEncode(...))` matches `keccak256(abi.encode(...))` on-chain.

## Consuming this module from an action

These files use **relative imports between each other** (`./lit-action-core.js`
etc.). The Lit Action runtime loads ES modules from jsDelivr by bare,
version-pinned specifier (see the existing `examples/solana-signer` action) and
does **not** resolve local relative imports. So the split set must be combined
before deploy. Pick one of:

1. **Bundle at deploy.** Bundle `lit-action-primitives.js` (or just the concerns
   you use) into your action with a build step, the way
   `examples/mpc-signing-frost` runs `npm run build:action` to inline its glue.
   The relative imports resolve at bundle time; the npm (`micro-eth-signer`,
   `@noble/*`) imports stay as runtime jsDelivr specifiers.
2. **Vendor it.** Copy the functions you use directly into your action file
   (collapsing the relative imports). Simplest; what most existing examples do
   (each inlines its own `rpcCall` / `ethCall`).
3. **Publish + pin.** Publish this package to npm and import a concern the same
   way the examples import `@noble/*`:
   `import * as P from "chipotle-primitives@x.y.z";`.

## Verify import resolution in the live runtime

The single most important thing to smoke-test: that every pinned specifier
resolves in **your** runtime. Run an action that imports this module and calls
`Lit.Actions.showImportDetails()` — it logs every resolved module URL and its
SHA-384. Confirm in particular:

- `micro-eth-signer@0.19.0` (root) resolves with its `@noble@2.2.0` +
  `micro-packed` deps bundled.
- `micro-eth-signer@0.19.0/advanced/abi.js` (the ABI subpath) resolves. If it
  does not, try the `/+esm` form (`micro-eth-signer@0.19.0/advanced/abi.js/+esm`)
  or vendor `createContract` in.
- `@noble/hashes@1.4.0/sha3/+esm` and `.../utils/+esm` resolve (this is the form
  already proven in `examples/solana-signer`).

`ethers` v5 is also available as a runtime global if you ever need a fallback
for the ABI coder; the rest of this module does not depend on it.

## Source files in `ext/js` vs this module

This module lives under `examples/`, not in `lit-actions/ext/js/`, so the
ASCII-only snapshot constraint does not strictly apply — but the file is kept
ASCII-only anyway. If you ever move it into the runtime snapshot, keep it ASCII
(use `\uXXXX` escapes for any non-ASCII).
