# Private Stablecoin (privUSD)

**A compliant private stablecoin. Balances and transfers are hidden on-chain;
the issuer can prove reserves and a regulator can decrypt one transaction with
a warrant — no ZK circuits.**

Every public stablecoin (USDC, USDT, PYUSD) puts your whole financial life on a
public ledger: payroll, vendor payments, who paid whom and how much, forever.
Shielded pools (Zcash, Aztec) fix the privacy but have no compliance story, so
issuers won't touch them. This example is the missing middle: **private by
default, compliant by construction.**

> **Demo-grade.** Runs on Base Sepolia with a mock USDC. The cryptography and
> trust model are real, but several pieces are simplified for clarity (noted
> throughout and listed under "What's demo-grade"). Production hardening is in
> the plan.

## The idea in one picture

```
        PUBLIC CHAIN (Base)                    LIT THRESHOLD NETWORK (TEEs)
  ┌───────────────────────────┐          ┌────────────────────────────────────┐
  │ PrivUSD.sol                │          │ action/ledger.js  (the "prover")     │
  │  • commitments (hashes)    │◄────┐    │  • reads chain state                 │
  │  • nullifiers (spent tags) │     │    │  • decrypts/validates notes in TEE   │
  │  • encrypted note blobs    │     ├────│  • checks sums, OFAC, KYC            │
  │    (in event logs)         │ sig │    │  • signs the state update            │
  │  • totalSupply (reserve pf)│     │    └────────────────────────────────────┘
  └───────────────────────────┘     │    ┌────────────────────────────────────┐
   no amounts, no parties — just     └────│ action/disclose.js                   │
   opaque hashes + ciphertext             │  • 3-of-5 warrant → decrypt ONE note │
                                          └────────────────────────────────────┘
```

A balance is a set of **notes** (`{owner, amount, salt}`). On-chain you only ever
see a note's **commitment** (`keccak256(owner, amount, salt)`), its **nullifier**
when spent, and its contents **encrypted** to a ledger PKP (decryptable only
inside an authorized Lit Action). The Lit Action plays the role a ZK circuit
plays in Zcash/Aztec — it proves the transfer is valid — but it's ~500 lines of
JavaScript instead of a circuit. **That swap is the whole pitch: Aztec-grade
privacy with no circuit team.**

## What the chain reveals vs. hides

| Public on chain | Hidden |
| --- | --- |
| Total supply (for reserve proof) | Individual balances (note amounts) |
| Commitments + nullifiers | Who paid whom |
| Mint/redeem amounts (the dollar edge) | Transfer amounts |
| Reserve held (`reserveBacked()`) | Note ↔ wallet mapping |

## Compliance model — identity at the edges, privacy in the middle

- **OFAC screening runs on every operation**, on every recipient — the same
  on-chain Chainalysis lookup as `compliance-transfer-gate`, baked in and
  non-optional.
- **KYC runs only at the dollar edge** (`mint`/`redeem`), never on private
  transfers. You verify once when dollars enter; your transfers stay private.
  Same as real money: you KYC to open a bank account, not for each payment.
- **KYC is an attestation, not a database.** A provider (Persona/Sumsub in
  production; a local signing key here) signs that a subject passed; the action
  verifies the signature. No PII is stored.

## Files

| Path | Purpose |
| --- | --- |
| `contracts/PrivUSD.sol` | The ledger: commitments, nullifiers, encrypted blobs, reserve proof. Trusts one signer — the ledger action. |
| `contracts/MockUSDC.sol` | Mintable 6-decimal stand-in for USDC on testnet. |
| `action/ledger.js` | The prover. `op = mint \| transfer \| redeem`: reads chain state, validates notes, OFAC/KYC, signs the update. |
| `action/disclose.js` | Warrant-gated disclosure. Verifies a 3-of-5 multi-sig, decrypts one named note. |
| `scripts/lib/notes.js` | Note crypto (commitment/nullifier). Kept byte-identical to the formulas in `ledger.js`. |
| `scripts/lib/litClient.js` | ABI fragments + `/lit_action` caller. |
| `scripts/setup.js` | One-shot: mint PKP, compute CIDs, wire the group + usage key, deploy contracts. |
| `scripts/deploy.js` | Hardhat deploy (MockUSDC + PrivUSD, pinning the action signer). |
| `scripts/demo.js` | The 2-minute scripted demo (mint → reserve proof → transfer → disclosure). |
| `test/privusd.test.js` | Full on-chain path with a local signer. No Lit network needed. |

## Quick start

```bash
npm install
npm test          # validates the contract + crypto offline, no keys needed
```

To run the live demo on Base Sepolia:

```bash
cp .env.example .env
# Fill in: LIT_API_KEY, DEPLOYER_PRIVATE_KEY (with Base Sepolia gas),
#          SCREENING_RPC_URL (eth-mainnet.g.alchemy.com), KYC_SIGNER_PRIVATE_KEY
#          (any `openssl rand -hex 32`)
npm run setup     # mints PKP, wires Lit, deploys contracts — writes results to .env
npm run demo
```

`npm run demo` walks through:

1. **KYC + mint** — Alice verifies once, deposits 1,000 USDC → a private
   1,000 privUSD note.
2. **Reserve proof** — `totalSupply` and `reserveBacked()` shown live.
3. **Shielded transfer** — Alice pays Bob 250. The script prints the tx's
   on-chain footprint: nullifiers + commitments, no amount, no parties.
4. **Disclosure** — a 3-of-5 warrant decrypts *only* Bob's note ($250); a
   2-of-5 warrant is rejected. Alice's change note stays dark.

## Trust model

Two cryptographic identities, mirroring `compliance-transfer-gate`:

- **The ledger action's CID-derived signer** is the contract's only authority.
  It comes from `Lit.Actions.getLitActionPrivateKey()`, derived from the
  action's IPFS CID. Edit the action by one byte → new CID → new signer
  address → the deployed contract rejects it. The action's source is the
  policy, and it's content-addressed.
- **The ledger PKP** is the encrypt/decrypt key for note contents. It never
  appears on-chain. Only actions authorized in the example's group can use it,
  so plaintext note contents never leave a Lit TEE.

Both RPCs the action depends on are pinned by hostname, not taken from the
caller: `ALLOWED_SCREENING_HOST` (Alchemy Ethereum-mainnet, for OFAC) and
`ALLOWED_CONTRACT_HOST` (Alchemy Base-Sepolia, for reading PrivUSD's
commitment/nullifier state). Pinning the contract RPC is load-bearing — a
caller-supplied RPC could otherwise feed the action fabricated "this note
exists and is unspent" answers and get a redeem signed against a note that was
never minted. See `compliance-transfer-gate`'s README for why a caller-supplied
chain id would be theater.

## What's demo-grade

Each of these is a deliberate simplification with a known production path
(detailed in [the plan](../../plans/private-stablecoin.md)):

- **No Merkle membership / ZK.** The action validates inputs by reading the
  contract's public `commitments`/`nullifiers` mappings (over the *pinned* RPC).
  Production uses an incremental Merkle tree. The action is trusted as prover —
  the TEE + threshold network is the trust anchor, not a succinct proof.
- **No dedicated spending key.** Spends are authorized by an EIP-191 signature
  from the note owner's wallet over the exact operation (the action recovers it
  and requires it to match each input note's owner), so knowing a note's opening
  is *not* enough to spend it — and a disclosed `{owner, amount, salt}` is not a
  bearer note. The residual: the nullifier is `keccak(owner, salt)`, and the
  sender knows both, so a sender can *link* the notes she sent as they're later
  spent (a privacy leak, not a theft vector). A production scheme derives the
  nullifier from a PRF under the recipient's secret spending key (Zcash/Aztec
  style) so even the sender can't link or spend.
- **Disclosure authority set is passed in `js_params`** (only a threshold floor
  is enforced in-action). Production pins the exact authority set on-chain or
  bakes it into the action's CID.
- **The Lit group uses a wildcard action allowlist** (`cid_hashes_permitted:
  ["0"]`, inherited from `compliance-transfer-gate`). Anyone holding the scoped
  usage key can run a custom action that calls `Lit.Actions.Decrypt` with the
  ledger PKP, bypassing `disclose.js`. Production pins only the ledger + disclose
  CIDs so only those actions can use the PKP. This is the main remaining gap
  between the demo and the "even the operator can't read balances" guarantee.
- **KYC attestation is an EIP-191 signed message** verified against an address
  in `js_params`. Production pins the provider's key via a hostname-anchored
  JWKS endpoint.
- **Single OFAC provider.** Production fans out to 2–3 (multi-source consensus,
  see `../multi-source-price-oracle`).
- **Disclosure returns plaintext over the response channel.** Production
  re-encrypts to the regulator's pubkey and logs the disclosure event
  (warrantHash, timestamp) on-chain for accountability.
- **MockUSDC** instead of canonical USDC; **single signer** instead of an
  oracle-rotation setter behind a multisig.

## Why this is the sales demo

It's the one artifact that closes a regulated buyer: a live system showing
private transfers *and* a warrant decrypting exactly one of them while the rest
stay dark — the capability no other private stablecoin can demonstrate. See the
"Sales collateral" section of the plan.
