# Multi-RPC Consensus Oracle

A Lit Action that reads the same EVM view function from three independent RPC
providers (Infura, Alchemy, QuickNode) in parallel and only signs the result
when all three agree byte-for-byte at the same block. The companion
`ConsensusOracle` contract stores those attested readings on-chain so other
contracts can consume them.

Use this pattern when a single RPC provider's view of the chain would be a
single point of failure — a buggy upgrade, a hijacked endpoint, a deliberately
fraudulent response. Three independent infrastructure paths have to all return
the same bytes before the action will sign anything.

## How it works

```
  caller              Lit Action               Infura   Alchemy   QuickNode
    │                     │                       │        │          │
    │ js_params           │                       │        │          │
    ├────────────────────►│                       │        │          │
    │                     │ Decrypt URLs via      │        │          │
    │                     │ decryptPkpId          │        │          │
    │                     │ whitelist hostnames   │        │          │
    │                     │                       │        │          │
    │                     │ eth_chainId +         │        │          │
    │                     │ eth_blockNumber       │        │          │
    │                     ├──────────────────────►│───────►│─────────►│
    │                     │◄──────────────────────┤◄───────┤◄─────────┤
    │                     │ pick min(tip) - lag   │        │          │
    │                     │                       │        │          │
    │                     │ eth_call +            │        │          │
    │                     │ eth_getBlockByNumber  │        │          │
    │                     ├──────────────────────►│───────►│─────────►│
    │                     │◄──────────────────────┤◄───────┤◄─────────┤
    │                     │ require: returnData = │        │          │
    │                     │          blockHash  = │        │          │
    │ sig, returnData     │ sign with             │        │          │
    │◄────────────────────┤ getLitActionPrivateKey│        │          │
    │                     │                       │        │          │
    │ ConsensusOracle.submit(sig, returnData, ...)         │          │
    ├───────────────────────────────────────────────────────────────► registry
    │                     │   ecrecover(sig)                          │
    │                     │     == action wallet ✓                    │
```

### Two cryptographic identities

The example uses two distinct keys for two different jobs.

**Action-derived signing key.** The signature `ConsensusOracle.submit` checks
comes from `Lit.Actions.getLitActionPrivateKey()` — a key derived
deterministically from the action's IPFS CID. The deployed registry pins the
*address* of that key as `signer`. If the action source changes by a single
byte the CID changes, the derived key changes, and old registries stop trusting
the new action. There is no way to produce a valid signature except by running
this exact code inside the Lit network.

**Decrypt PKP.** Lit's `Encrypt`/`Decrypt` API is PKP-keyed, so the three RPC
URLs (which embed API keys) are encrypted *to* some PKP. That PKP
(`DECRYPT_PKP_ADDRESS`) exists only as that encryption boundary; it signs
nothing the registry cares about.

### What stops a tampered caller from getting a forged reading signed

1. **Encrypted URLs.** The URLs are passed in as ciphertexts, decryptable
   only by the configured decrypt PKP.
2. **Hostname whitelist.** Anyone with a usage key can call
   `Lit.Actions.Encrypt` for this PKP and produce *some* ciphertext, so
   encryption alone doesn't prove a URL is one we trust. After decryption the
   action requires each host to match a regex for `*.infura.io`,
   `*.g.alchemy.com`, or `*.quiknode.pro`. Changing that list mints a new
   action CID *and* a new signer address — the registry would refuse the
   modified action's signatures.
3. **Cross-provider equality.** Both the return data *and* the canonical
   block hash at the read block must match across all three providers. Two
   colluding providers still can't fabricate a block hash the third would
   confirm.

## Files

| Path | Purpose |
| --- | --- |
| `action/consensusOracle.js` | The Lit Action: decrypt, whitelist, fetch in parallel, sign on agreement. |
| `contracts/ConsensusOracle.sol` | On-chain registry storing the signed readings. |
| `scripts/setup.js` | One-shot setup: mints the decrypt PKP, computes the action CID, derives the action's wallet address, creates and wires the group, deploys the registry, encrypts the three RPC URLs. Idempotent. |
| `scripts/mintPkp.js` | Mints a fresh decrypt PKP (also called by setup). |
| `scripts/deploy.js` | Hardhat deploy; pins the action's derived wallet address as `signer` (also called by setup). |
| `scripts/encryptRpcUrls.js` | Encrypts the three RPC URLs to the decrypt PKP (also called by setup). |
| `scripts/submit.js` | End-to-end runner: ask the action to read `balanceOf(holder)` of a token, then submit the signed reading to the registry. |
| `scripts/test-consensus.js` | Zero-dep harness for the multi-RPC consensus logic — no Lit envelope, no PKP signing, just runs the equality checks against plaintext RPC URLs from `.env`. Useful before you have all three provider keys. |
| `scripts/_env.js` | Tiny shared helper: reads `.env` and upserts new lines into it. |
| `.env.example` | All the env vars you'll fill in. |

## Walkthrough

### 1. Fill in your inputs

```bash
cp .env.example .env
npm install
```

Edit `.env` and set:
- `LIT_USAGE_API_KEY` — from the [Chipotle dashboard](https://dashboard.chipotle.litprotocol.com)
- `INFURA_URL`, `ALCHEMY_URL`, `QUICKNODE_URL` — one full RPC URL each (free tiers are fine)
- `DEPLOYER_PRIVATE_KEY` — an EOA with gas on Base Sepolia (or your target chain)

Everything else (`DECRYPT_PKP_ADDRESS`, `ACTION_IPFS_CID`,
`ACTION_WALLET_ADDRESS`, `GROUP_ID`, `CONSENSUS_ORACLE_ADDRESS`, and the three
`ENCRYPTED_*_URL` ciphertexts) is derived by the setup script and written
back to `.env` for you.

### 2. Run setup

```bash
npm run setup
```

This walks through nine steps, printing each one as it goes:

1. Mint the decrypt PKP (encryption boundary for the URLs).
2. Compute the action's IPFS CID.
3. Derive the action's wallet address from its CID — this is what the
   registry will pin as `signer`.
4. Create a permission group.
5. Register the action against your account.
6. Authorize the action inside the group.
7. Authorize the decrypt PKP inside the group.
8. Deploy `ConsensusOracle` with the action's wallet address as `signer`.
9. Encrypt the three RPC URLs to the decrypt PKP.

Every step that produces a new value writes it back to `.env`, so re-runs
skip whatever's already done. If you edit the action source, step 2 detects
the new CID, clears the now-stale `ACTION_WALLET_ADDRESS` and `GROUP_ID`,
and re-runs steps 3–7 with the fresh CID.

If you'd rather do steps manually, each one is its own npm script; the
[scripts/setup.js source](./scripts/setup.js) is heavily commented and meant
to be read as a template.

If you only have one of the three provider keys, you can still validate the
consensus logic separately with `npm run test-consensus` — see the bottom of
this README for details.

### 3. Submit a consensus reading

The example reads ERC-20 `balanceOf(holder)` for any token + holder you point
it at:

```bash
npm run submit -- --token 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48 \
                  --holder 0x28C6c06298d514Db089934071355E5743bf21d60
```

Expected output:

```
Consensus reached at block 19874321: balance = 4500000000
tx: 0x...
mined in block 12345678
```

If any provider disagrees, the action returns `{ authorized: false, reason: ... }`
without signing — the on-chain step is never reached.

## Reading the oracle from another contract

```solidity
interface IConsensusOracle {
    function latest(address target, bytes calldata callData)
        external
        view
        returns (bytes memory data, uint64 observedAt, uint64 submittedAt);
}

contract MyConsumer {
    IConsensusOracle constant ORACLE = IConsensusOracle(0x...);

    function getBalance(address token, address holder) external view returns (uint256) {
        bytes memory callData = abi.encodeWithSignature("balanceOf(address)", holder);
        (bytes memory data, uint64 observedAt, ) = ORACLE.latest(token, callData);
        require(block.timestamp - observedAt < 1 hours, "stale reading");
        return abi.decode(data, (uint256));
    }
}
```

The consumer enforces its own staleness window — the registry only tells you
when the reading was observed.

## Tuning

- **`blockLagBlocks`** (default 5). The action reads at `min(tip across all
  providers) - lag` so that fast providers don't get a `block not found` error
  from slower ones. Tune this for your chain's reorg depth.
- **Quorum.** This example requires unanimous agreement among three providers.
  Relaxing to 2-of-3 is a small change (group `returnDatas` by value, require
  a majority bucket) and is a reasonable choice when one provider being
  briefly offline is more likely than two providers colluding.
- **Source chain.** The action verifies each RPC reports `sourceChainId` via
  `eth_chainId` before doing anything else. Swap the chain freely without
  touching the contract.

## Production considerations

- **Replay.** The contract enforces `observedAt` is strictly increasing per
  `(target, callData)`. A submitted signature is therefore only useful once.
- **Liveness.** If one provider is down, the action fails closed. If you'd
  rather fail open with a 2-of-3 quorum, change the equality check in
  `consensusOracle.js` as noted above.
- **Whitelist drift.** The hostname regexes hard-code provider DNS patterns.
  If a provider changes its DNS scheme you'll need a new CID and a group
  re-authorization. That's a feature: policy upgrades are explicit, not
  surprises pushed from a control plane.
- **Cost.** Each call pays gas only for the on-chain `submit` (a single
  `SSTORE` plus event); the multi-RPC read is off-chain and free.
