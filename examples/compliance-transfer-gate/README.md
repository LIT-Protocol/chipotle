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
   user wallet         Lit Action            Chainalysis oracle    CompliantToken
                                             (Ethereum mainnet)        (Base)
       │                   │                         │                   │
       │ js_params         │                         │                   │
       ├──────────────────►│                         │                   │
       │                   │ eth_chainId             │                   │
       │                   │ (defensive)             │                   │
       │                   ├────────────────────────►│                   │
       │                   │◄────────────────────────┤                   │
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
- `LIT_USAGE_API_KEY` — from the [Chipotle dashboard](https://dashboard.chipotle.litprotocol.com)
- `DEPLOYER_PRIVATE_KEY` — an EOA with gas on Base Sepolia (or your target chain)
- `SENDER_PRIVATE_KEY` — an EOA holding tokens (typically the same as deployer for testing)

That's it. No Chainalysis signup, no encryption keys. The default
`SCREENING_RPC_URL` points at `eth.drpc.org` (Ethereum mainnet) — change it
if you'd rather use your own RPC. To target a different chain, change
`DEPLOY_NETWORK` (Hardhat network name) and `CHAIN_ID` + `RPC_URL` (where
the token lives).

### 2. Run setup

```bash
npm run setup
```

This walks through six steps, printing each one as it goes:

1. Compute the action's IPFS CID.
2. Derive the action's wallet address from its CID — this is what the
   contract will pin as `complianceOracle`.
3. Create a permission group.
4. Register the action against your account.
5. Authorize the action inside the group.
6. Deploy `CompliantToken` with the action's wallet address as the oracle.

Every step that produces a new value writes it back to `.env`. If you edit
the action source, step 1 detects the new CID, clears the now-stale
`ACTION_WALLET_ADDRESS` and `GROUP_ID`, and re-runs steps 2–5 with the fresh
CID. The on-chain contract still trusts the old address — you'd redeploy
the token (or wire in a rotate-oracle path) after such an edit.

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

The current action trusts a single screening RPC. A compromised RPC could lie
about `isSanctioned`. To eliminate that single point of failure, apply the
multi-source pattern used in [`../multi-source-price-oracle`](../multi-source-price-oracle):
fan out the `eth_call` to two or three independent mainnet RPCs and only sign
when they all return the same `isSanctioned` byte. The defensive `eth_chainId`
check in this action is a small step toward that; full multi-source agreement
is a much stronger guarantee.

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
