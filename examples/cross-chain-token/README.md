# Cross-Chain Token

**Permissionless burn/mint bridging for an ERC-20: a Lit Action checks burn
events on one chain and signs the matching mint on another. No relayer
trust, no bridge multisig, no API key — works for any pair of chains the
action knows how to read.**

Same `BridgeToken` contract is deployed on two chains (Base Sepolia and
Arbitrum Sepolia by default). To move tokens A → B, the holder calls `burn`
on A, which destroys the local supply and emits `BurnInitiated`. The Lit
Action reads that event over a whitelisted RPC, validates it, and signs a
mint authorization for B. Anyone can submit the mint — the signature is the
authorization, not the caller.

## How it works

```
   Alice (wallet)      BridgeToken (Base Sepolia)   Lit Action     BridgeToken (Arb Sepolia)
                                                                       ┃
       │                       ┃                       │               ┃
       │  burn(amount, 421614, recipient)               │               ┃
       ├──────────────────────►┃                        │               ┃
       │                       ┃ _burn(alice, amount)   │               ┃
       │                       ┃ emit BurnInitiated     │               ┃
       │                       ┃                        │               ┃
       │ /lit_action with      │                        │               ┃
       │ { burnTxHash, …}      │                        │               ┃
       ├────────────────────────────────────────────────►               ┃
       │                       │                        │               ┃
       │                       │ eth_getTransactionReceipt              ┃
       │                       │ (whitelisted Alchemy host, chainId 84532)
       │                       │ ◄──────  decode BurnInitiated, sign    ┃
       │   { signature, … }    │                        │               ┃
       │◄────────────────────────────────────────────────               ┃
       │                       │                        │               ┃
       │  mint(srcChainId, srcContract, burnTxHash, logIndex,           ┃
       │       recipient, amount, srcNonce, deadline, sig) ────────────►┃
       │                       │                        │ ecrecover     ┃
       │                       │                        │  == bridgeOracle ✓
       │                       │                        │ bridgePartner[84532] == src ✓
       │                       │                        │ !usedBurnIds  ┃
       │                       │                        │  _mint(recipient, amount)
```

### Why this is a Lit-shaped problem

A token contract on chain B can't read state from chain A. Most bridges
solve that with a federation of off-chain signers — a multisig, an LP set,
or a centralized operator — who watch the source chain and attest to the
destination. The trust assumption is "this group of signers is honest /
quorate."

A Lit Action collapses that down to "this exact piece of code, content-
addressed by its IPFS CID, is honest." The signer key comes from
`Lit.Actions.getLitActionPrivateKey()`, which derives the key from the
action's CID. Edit the action by a byte and the CID changes, the signer
address changes, and every deployed `BridgeToken` refuses signatures from
the modified action. The trust assumption is now "the published action
source does what it says, and TLS to the named RPC isn't compromised" —
which is a much smaller surface.

## Files

| Path | Purpose |
| --- | --- |
| `action/bridgeAction.js` | The Lit Action: validates a burn receipt on the source chain and signs a mint authorization for the destination chain. |
| `contracts/BridgeToken.sol` | ERC-20 with `burn` (announces a destination + amount) and `mint` (verifies the action's signature). The same contract is deployed on every chain. |
| `scripts/setup.js` | One-shot: action CID + group + scoped key, deploy on both chains, wire each chain's `bridgePartner` to point at the other. |
| `scripts/deploy.js` | Hardhat deploy for one chain at a time. Called by setup.js for each chain. |
| `scripts/bridge.js` | End-to-end client: burn → ask action → mint. |
| `scripts/_env.js` | Tiny shared `.env` reader / upserter. |
| `.env.example` | All the env vars you'll fill in. |

## Walkthrough

### 1. Fill in your inputs

```bash
cp .env.example .env
npm install
```

Edit `.env` and set:
- `LIT_API_KEY` — your **account-level (master) API key** from the
  [Chipotle dashboard](https://dashboard.chipotle.litprotocol.com). Setup
  calls management endpoints that scoped keys can't.
- `DEPLOYER_PRIVATE_KEY` — an EOA with gas on **both Base Sepolia and
  Arbitrum Sepolia**. Base Sepolia ETH from any public faucet; bridge a
  little over to Arb Sepolia via the
  [Arbitrum bridge UI](https://bridge.arbitrum.io).
- `BASE_SEPOLIA_ALCHEMY_URL` and `ARBITRUM_SEPOLIA_ALCHEMY_URL` — Alchemy
  RPC URLs for both chains. Free tier at
  [dashboard.alchemy.com](https://dashboard.alchemy.com) works; add both
  networks to a single app and copy the two URLs.

### Trust model

A naïve version of this action might accept `srcRpcUrl` from the caller and
trust whatever it reports. That's broken: anyone with the usage API key
could supply a hostile RPC that returns a fake "BurnInitiated" log for a
burn that never happened, and the action would happily sign a mint.

The fix is the same as in [`compliance-transfer-gate`](../compliance-transfer-gate):
the action enforces a **per-chain hostname whitelist** baked into the
source. For `srcChainId=84532` (Base Sepolia) the URL must resolve to
`base-sepolia.g.alchemy.com`; for `srcChainId=421614` (Arb Sepolia) it
must be `arb-sepolia.g.alchemy.com`. Hostnames are anchored
(`^`/`$`) so subdomain tricks like `base-sepolia.g.alchemy.com.attacker.com`
get rejected. TLS guarantees the body came from the named host, so the
trust shifts onto "Alchemy is reporting the chain truthfully" — the same
assumption almost every dapp already makes.

A belt-and-suspenders `eth_chainId` check runs after the hostname check.

To use a different provider, edit `RPC_HOSTS` in
[`action/bridgeAction.js`](./action/bridgeAction.js). For Infura use
`/^arbitrum-sepolia\.infura\.io$/i`, for QuickNode use
`/^[a-z0-9-]+\.quiknode\.pro$/i`, etc. Any edit changes the action's
IPFS CID and therefore the signer address, so every existing
`BridgeToken` deployment will refuse signatures from the modified action
— you'd need to redeploy.

### 2. Run setup

```bash
npm run setup
```

This walks through nine steps:

1. Compute the action's IPFS CID.
2. Create a permission group with a wildcard action allowlist
   (`cid_hashes_permitted: ["0"]`). The wildcard is what makes the one-shot
   address-deriver action in step 4 executable — its inline CID isn't
   registered anywhere.
3. Create a scoped usage API key with `execute_in_groups: [groupId]`,
   saved as `LIT_USAGE_API_KEY` in `.env`. `bridge.js` later calls
   `/lit_action` with this key — the master key can't execute actions in
   your own groups.
4. Derive the action's wallet address from its CID. This is what each
   `BridgeToken` will pin as `bridgeOracle`.
5. Register the action against your account (metadata).
6. Add the specific action CID to the group (audit trail).
7. Deploy `BridgeToken` on Base Sepolia (mints `INITIAL_SUPPLY` to the
   deployer if Base Sepolia is your `INITIAL_SUPPLY_NETWORK`, which it
   is by default).
8. Deploy `BridgeToken` on Arbitrum Sepolia (starts with zero supply —
   tokens get there by bridging).
9. Call `setBridgePartner(otherChainId, otherAddress)` on each deployment
   to pin its sibling. Done from JS because each side needs to know the
   other's deployed address, which doesn't exist before its own deploy.

Re-running `npm run setup` does a fresh setup top-to-bottom — every step
mints new state and overwrites the corresponding key in `.env`. The
previously-minted group / usage key / contracts become orphaned. Fine for
an example; a production deployment would version and rotate.

### 3. Bridge tokens

```bash
# Base Sepolia → Arbitrum Sepolia
npm run bridge -- --from baseSepolia --to arbitrumSepolia --amount 25 \
  --recipient 0xYourAddress

# Reverse direction (after you have a balance on Arb Sepolia)
npm run bridge -- --from arbitrumSepolia --to baseSepolia --amount 10 \
  --recipient 0xYourAddress
```

Expected output:

```
Step 1/3: Burning 25 tokens on Base Sepolia (chainId 84532)...
  burn tx: 0x...
  mined in block 12345678
  BurnInitiated nonce=1 logIndex=3

Step 2/3: Asking Lit Action to attest the burn...
  signature: 0x9f8c2d1e...
  signer:    0x1234...beef

Step 3/3: Minting on Arbitrum Sepolia (chainId 421614)...
  mint tx: 0x...
  mined in block 87654321

✓ Bridged 25 tokens from Base Sepolia to Arbitrum Sepolia.
  Recipient 0xYourAddress balance on Arbitrum Sepolia: 25.0
```

## Targeting different chains

The example defaults to Base Sepolia ↔ Arbitrum Sepolia. To add or swap
chains, three places change in lockstep:

1. **Hardhat config** ([`hardhat.config.js`](./hardhat.config.js)): add the
   network and its RPC + chainId.
2. **Action** ([`action/bridgeAction.js`](./action/bridgeAction.js)): add
   the chain id and its allowed RPC hostname to `RPC_HOSTS`. **Editing the
   action changes its CID, which changes the signer address — you must
   redeploy every existing `BridgeToken` afterwards.** That's the trust
   property at work.
3. **Setup + bridge scripts** ([`scripts/setup.js`](./scripts/setup.js),
   [`scripts/bridge.js`](./scripts/bridge.js)): add the chain to `CHAINS` /
   `NETWORKS` so the deploys and per-chain wiring know about it.

The "burn-on-A-mint-on-B" pattern itself doesn't care how many chains are
involved — the action signs `(srcChainId, srcContract, …, destContract,
destChainId)` so the destination contract independently verifies which
source it expected.

## Production considerations

- **Finality.** This example signs immediately after `eth_getTransactionReceipt`
  returns. On chains with reorg risk (most rollups settle in seconds, but
  L1 has minutes of finality) you'd add a confirmations check inside the
  action — `eth_getTransactionByHash` against `latest - N` blocks, or wait
  for an explicit `finalized` block tag. The action's `RPC_HOSTS` whitelist
  ensures the block tag genuinely comes from the host you named.
- **Double-mint protection.** Each `(srcChainId, burnTxHash, logIndex)` is
  recorded in `usedBurnIds[bytes32]`, so even if a relayer replays the
  signed mint, the second attempt reverts with `AlreadyRedeemed`.
- **Locked supply.** The destination's `_mint` is gated only by the signed
  attestation — no peg, no LP, no cap. Conservation across chains is
  enforced by the action's logic: it only signs after observing an
  on-chain burn. If you swap the action for one that signs without
  reading the burn, supply across the bridge would diverge.
- **Pause / upgrade.** `BridgeToken` is intentionally minimal — no pause,
  no admin keys beyond the one-time `setBridgePartner` wiring. A
  production deployment would wrap `mint` in a pause guard controlled by
  a multisig, and consider a per-block mint cap.
- **Action CID rotation.** Once tokens exist across both chains, rotating
  the action requires redeploying every `BridgeToken` (the oracle address
  is `immutable`). A production version would store `bridgeOracle` in
  storage and add a multisig-gated `setBridgeOracle` so policy upgrades
  don't require new token contracts and migrations.
- **Fees.** Anyone can submit the mint, so a relayer business could sponsor
  gas in exchange for a fee (skim it from `amount` in the action). This
  example transfers the full amount because gas-sponsoring is its own
  topic.
