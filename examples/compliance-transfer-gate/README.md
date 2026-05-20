# Compliance Transfer Gate

**OFAC sanctions screening for ERC-20 transfers on any chain — including chains
Chainalysis doesn't deploy on.**

The Chainalysis on-chain sanctions oracle is keyless and free, but it's only
deployed on a handful of mainnets — Ethereum, Arbitrum, Polygon, BSC,
Avalanche, Optimism, Celo. **It is not on Base.** Or Linea. Or Scroll. Or any
L3. Or any testnet. On those chains, a contract can't reach Chainalysis with a
plain `staticcall`.

This example uses a Lit Action to bridge that gap. The action runs an
`eth_call` against the Chainalysis oracle on Ethereum mainnet, then signs an
authorization that the `CompliantToken` contract — deployed wherever you want
— can verify with `ecrecover`. No bridge, no API key, no minted PKP, no
encrypted secrets. The default target is Base Sepolia; you can swap it for any
EVM chain Hardhat can talk to.

## How it works

```
   user wallet         Lit Action          Alchemy + Chainalysis     CompliantToken
                                             (Ethereum mainnet)         (Base)
       │                   │                         │                   │
       │ js_params         │                         │                   │
       ├──────────────────►│                         │                   │
       │                   │ check URL host          │                   │
       │                   │ matches alchemy regex   │                   │
       │                   │                         │                   │
       │                   │ eth_call(               │                   │
       │                   │   chainalysis,          │                   │
       │                   │   isSanctioned(to))     │                   │
       │                   ├────────────────────────►│                   │
       │                   │◄────────────────────────┤                   │
       │                   │  bool                   │                   │
       │ sig, returnData   │ if !sanctioned: sign    │                   │
       │◄──────────────────┤ with                    │                   │
       │                   │ getLitActionPrivateKey()│                   │
       │                   │                                             │
       │ transferWithAuth(to, amount, nonce, deadline, sig) ────────────►│
       │                   │                            ecrecover(sig)   │
       │                   │                              == oracle ✓    │
       │                   │                              _transfer()    │
```

### Why this is a Lit-shaped problem

You could deploy CompliantToken on Ethereum, where Chainalysis lives, and
call `isSanctioned` directly from a `_beforeTokenTransfer` hook — no Lit
needed. That's exactly the right answer there.

But if your token is on **Base**, or **Solana** (once non-EVM PKPs are in
play), or a custom L2, or testnets — there is no on-chain Chainalysis. Your
options become: (a) operate a server that holds an API key and trust it; (b)
build a bridge; or (c) put a Lit Action in front of the oracle, sign an
attestation, and let your contract verify it. This is option (c).

The signature comes from `Lit.Actions.getLitActionPrivateKey()` — a key
derived deterministically from the action's IPFS CID. The deployed
`CompliantToken` pins the address of that key. Edit the action by a byte and
the CID changes, the key changes, the address changes — and the contract
stops trusting the modified action.

## Files

| Path | Purpose |
| --- | --- |
| `action/complianceGate.js` | The Lit Action: probes screening RPC for chain id, eth_calls Chainalysis `isSanctioned`, signs the authorization. |
| `contracts/CompliantToken.sol` | ERC-20 with `transferWithAuth` that verifies the action's signature against the pinned oracle address. |
| `scripts/setup.js` | One-shot setup: computes the action CID, derives the action's wallet address, creates and wires the group, deploys the contract. Idempotent. |
| `scripts/deploy.js` | Hardhat deploy script; pins the action's derived wallet address as the on-chain oracle (also called by setup). |
| `scripts/transfer.js` | End-to-end client that calls the Lit Action, then submits the on-chain transfer. |
| `scripts/_env.js` | Tiny shared helper: reads `.env` and upserts new lines into it. |
| `.env.example` | All the env vars you'll fill in. |

## Walkthrough

### 1. Fill in your inputs

```bash
cp .env.example .env
npm install
```

Edit `.env` and set:
- `LIT_API_KEY` — your **account-level (master) API key** from the
  [Chipotle dashboard](https://dashboard.chipotle.litprotocol.com), *not* a
  scoped usage key. Setup calls management endpoints (`/add_action`,
  `/add_group`) that revert `NotMasterAccount` on scoped keys.
- `DEPLOYER_PRIVATE_KEY` — an EOA with gas on Base Sepolia (or your target chain)
- `SENDER_PRIVATE_KEY` — an EOA holding tokens (typically the same as deployer for testing)
- `SCREENING_RPC_URL` — an **Ethereum mainnet Alchemy URL** of the form
  `https://eth-mainnet.g.alchemy.com/v2/<your-api-key>`. Free tier at
  https://dashboard.alchemy.com works. The action hardcodes a hostname
  whitelist requiring `eth-mainnet.g.alchemy.com` (see the "Trust model"
  section below).

No Chainalysis signup, no encryption keys. To target a different *token*
chain (where `CompliantToken` lives), change `DEPLOY_NETWORK` (Hardhat
network name) and `CHAIN_ID` + `RPC_URL`. The *screening* chain stays
on Ethereum mainnet unless you also edit the action.

### Trust model

A naïve version of this action might take both a `screeningRpcUrl` and a
`screeningChainId` and check that `eth_chainId` reports the expected value.
That check is theater: anyone calling the action supplies both fields, so
they can lie consistently — pair a malicious RPC with a matching chain id
and the gate passes.

The real anchor is the hostname. The action checks
`new URL(screeningRpcUrl).hostname` against an anchored regex
(`/^eth-mainnet\.g\.alchemy\.com$/`). The check passes only when TLS
delivers data from Alchemy's actual servers — which is something nobody
short of a CA-level compromise can fake. Trust shifts to "Alchemy is
honest about Ethereum mainnet state," which is the same assumption almost
every dapp already makes.

To swap providers, edit `ALLOWED_SCREENING_HOST` in
[`action/complianceGate.js`](./action/complianceGate.js). For Infura use
`/^mainnet\.infura\.io$/i`, for QuickNode use
`/^[a-z0-9-]+\.quiknode\.pro$/i`, etc. Any edit changes the action's
IPFS CID and therefore its signer address — the existing on-chain
contract will refuse signatures from the modified action, so you'd need
to redeploy (or wire a rotate-oracle setter behind a multisig).

### 2. Run setup

```bash
npm run setup
```

This walks through seven steps, printing each one as it goes:

1. Compute the action's IPFS CID.
2. Create a permission group with a **wildcard action allowlist**
   (`cid_hashes_permitted: ["0"]` → `U256(0)` in the on-chain `cidHash` set,
   which the contract reads as "any action allowed here"). Wildcard is
   what makes the one-shot deriver action in step 4 executable — its
   inline CID isn't registered anywhere.
3. Create a scoped usage API key with `execute_in_groups: [groupId]`,
   saved as `LIT_USAGE_API_KEY` in `.env`. Both the deriver in step 4
   and `transfer.js` later call `/lit_action` with this key — the master
   key can't execute actions in your own groups (the contract's
   `canExecuteAction` only consults the `usageApiKeys[...]` mapping).
4. Derive the action's wallet address from its CID — this is what the
   contract will pin as `complianceOracle`. Runs via `/lit_action` with
   the scoped usage key.
5. Register the action against your account (metadata only).
6. Add the specific action CID to the group (audit trail; technically
   redundant given the wildcard but lets you later tighten the group
   by removing the wildcard via `update_group`).
7. Deploy `CompliantToken` with the action's wallet address as the oracle.

Re-running `npm run setup` does a fresh setup top-to-bottom: every step
creates new on-chain state and overwrites the corresponding key in
`.env`. The previously-minted group / usage key / contract become
orphaned. That's intentional — a production app would manage upgrades,
but here "run setup again" is the simplest reset.

### 3. Send a compliance-gated transfer

```bash
# Clean address — passes
npm run transfer -- --to 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045 --amount 100

# Chainalysis's documented test sanctioned address — fails closed
npm run transfer -- --to 0x7F367cC41522cE07553e823bf3be79A889DEbe1B --amount 100
```

Expected output for a clean recipient:

```
Recipient cleared sanctions screening. Submitting transfer...
tx: 0x...
mined in block 12345678
```

Expected output for the sanctioned test address:

```
Lit Action denied the transfer: {
  authorized: false,
  reason: 'Recipient is on the Chainalysis sanctions oracle'
}
```

## Targeting a different chain

The example defaults to deploying `CompliantToken` on Base Sepolia. To use a
different chain:

- Add a network to `hardhat.config.js` (the file already includes `base` and
  `baseSepolia`).
- Set `DEPLOY_NETWORK` to the Hardhat network name.
- Set `CHAIN_ID` and `RPC_URL` to match — these are what the *Lit Action* and
  *transfer.js* use to talk to your deployed contract.
- Leave `SCREENING_RPC_URL` and `SCREENING_CHAIN_ID` alone — the action always
  reads Chainalysis on Ethereum mainnet (or whatever screening chain you point
  it at), independent of where the token lives.

This is the cross-chain bit: the screening chain and the token chain are
decoupled.

## Hardening: multi-source consensus

The current action trusts one provider (Alchemy). If Alchemy itself ever lied
about an `isSanctioned` reading — buggy upgrade, hijacked endpoint, deliberate
fraud — the gate would happily sign a malicious transfer. To eliminate that
single point of failure, apply the multi-source pattern used in
[`../multi-source-price-oracle`](../multi-source-price-oracle): fan the
`eth_call` out to two or three independently-hostnamed providers (Alchemy +
Infura + QuickNode) and only sign when they all return the same bool. Edit
the action to keep three regexes instead of one and require all three URLs
in `js_params`.

## Production considerations

- **Replay protection.** The contract stores `(from, nonce) → used`. The
  client picks a random 32-byte nonce per call; switch to a counter if you
  prefer deterministic nonces.
- **Deadline.** The action sets `deadline = now + 10 min`. Tighten or relax
  to match your settlement window.
- **Oracle freshness.** Chainalysis publishes new sanctioned addresses to
  their oracle on roughly a daily cadence. There is a window — measured in
  minutes to hours — where the OFAC SDN list contains an address that the
  on-chain oracle has not yet acknowledged. Decide whether your product
  needs to bridge that gap (e.g. by combining with a paid API).
- **Policy upgrades.** Because the oracle address is derived from the
  action's CID, any change to the action source produces a new oracle
  address. Old `CompliantToken` deployments will refuse signatures from the
  new action; the upgrade path is either (a) redeploy the token with the
  new oracle, or (b) add a setter behind a multisig that rotates
  `complianceOracle` to the new derived address.
- **Coverage caveat.** Chainalysis's on-chain oracle only screens against
  OFAC SDN and similar lists. It will not flag a wallet that's known to be
  a drainer or mixer counterparty unless that wallet is *also* on a
  sanctions list. For richer screening, swap the on-chain lookup for a
  paid provider (TRM Labs, GetBlock, MetaSleuth, Chainalysis KYT) — the
  pattern (encrypt API key → decrypt in TEE → fetch → sign) is shown in
  other examples.
