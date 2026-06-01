# Action-Bound Wallet

**A unique, immutable wallet for every user — bound to a Lit Action's code, with
no PKP to mint and no contract to deploy. You get the per-user wallet by
stamping the user's address into the action; the action's CID does the rest.**

This is the answer to a common question: *"how do I bind a wallet to an action
immutably, and give each of my users their own?"* The usual route — convert to a
ChainSecured account and use a contract to create and bind PKPs to a group — is
powerful but a lot of setup. This example shows a lighter pattern that covers
the same "wallet that can only be used by this exact code" property.

## The idea

Every Lit Action has a key derived from its IPFS CID
(`Lit.Actions.getLitActionPrivateKey()`). That key only exists while that exact
code runs inside the Lit network — there's no way to export it. So the wallet is
**bound to the code**: change a byte, the CID changes, the key changes, the
address changes.

To get a *different wallet per user*, make the code different per user. The
cheapest way to do that is to **hardcode the user's address into the action**:

```javascript
const OWNER_ADDRESS = "0xUSERS_ADDRESS"; // stamped in per user
```

That one line is part of what the CID hashes. Two users → two source files →
two CIDs → two wallets. One template, one immutable wallet each.

Then authorize spending by **recovering a signature** inside the action and
comparing it to the hardcoded `OWNER_ADDRESS`. The action signs a withdrawal
only when the request carries the owner's signature over the exact
`(token, to, amount, nonce, deadline)` tuple. Crucially, the Lit usage key that
*runs* the action grants no spending power — anyone can run anyone's action, but
they can only read the deposit address or relay a withdrawal the real owner
already signed.

```
   client (owner EOA)        Lit Action (userWallet, OWNER baked in)        chain
        │                          │                                          │
        │ action:"address"         │ derive wallet from CID                   │
        ├─────────────────────────►│                                          │
        │◄─────────────────────────┤ walletAddress                            │
        │  deposit ERC-20 + gas ───────────────────────────────────────────► │
        │                          │                                          │
        │ sign (token,to,amount,   │                                          │
        │       nonce,deadline)    │                                          │
        │ action:"withdraw" + sig  │ recover(sig)==OWNER_ADDRESS ?            │
        ├─────────────────────────►│ read nonce/gas, sign ERC-20 transfer    │
        │◄─────────────────────────┤ rawTx                                    │
        │  eth_sendRawTransaction ─────────────────────────────────────────► │
```

## When to use this vs. ChainSecured + PKP groups

| | Action-bound wallet (this example) | ChainSecured account + PKP group |
| --- | --- | --- |
| Setup | Stamp an address into one action template | Convert account, deploy/configure a binding contract |
| Wallet identity | Derived from the action CID (code-bound) | A minted PKP bound to the action via the group |
| "Only this code can sign" | ✅ by construction (key *is* the CID's key) | ✅ enforced by the group's action allowlist |
| Per-user isolation | A different CID per user | A different PKP per user |
| Best when | You want many lightweight, code-bound wallets fast | You need PKP features (rotation, multi-action groups, on-chain ownership) |

Both give you "this wallet can only be used by this exact action." This pattern
trades the PKP's flexibility for near-zero setup.

## Why the binding holds

- **Code-bound key.** The wallet's private key is derived from the action's CID
  and never leaves the Lit TEE. There is no key file to steal; the only way to
  produce a signature is to run this exact code in the network.
- **User-bound authority.** Spending requires a signature that recovers to the
  `OWNER_ADDRESS` hardcoded in the code. The usage key can run the action but
  cannot spend.
- **No cross-user reach.** Each user's action has a different CID and therefore a
  different wallet. There is no code path from one user's action to another's
  balance.
- **Replay-safe withdrawals.** The owner signs over `(wallet, chainId, token,
  to, amount, nonce, deadline)`. The action reads the wallet's nonce from a
  **pinned** RPC (see below) and requires the signed `nonce` to equal it, so a
  used authorization is dead the moment its tx lands; `deadline` bounds how long
  the authorization can be turned into a signed tx.
- **Pinned RPC host.** The action reads nonce + gas price from the caller's
  `rpcUrl`, so it pins the host (`ALLOWED_RPC_HOST` in `userWallet.js`). Without
  that, a caller could point the action at a hostile node that returns a bogus
  gas price (burning the wallet's native gas on broadcast) or a future nonce
  (so the signed tx lingers and executes later). Editing the host changes the
  CID and therefore every derived wallet — the same immutability that binds the
  owner. A gas-price cap is enforced too, as defense-in-depth.

## Files

| Path | Purpose |
| --- | --- |
| `action/userWallet.js` | The per-user action **template**. Holds the `__OWNER_ADDRESS__` placeholder, derives the wallet from the CID, and gates `withdraw` on the owner's signature. |
| `contracts/DemoToken.sol` | Plain ERC-20 with an open faucet `mint` so the demo has something to move. |
| `scripts/_users.js` | The three public Hardhat test keys used as demo users, and the function that stamps an address into the template to make a user's action source. |
| `scripts/_canonical.js` | The canonical withdrawal message — kept identical to the action's copy so signatures verify. |
| `scripts/_lit.js` | Runs a given user's action against the Lit API with the scoped usage key. |
| `scripts/setup.js` | One-shot: create the wildcard group, mint a scoped usage key, deploy the token. |
| `scripts/address.js` / `deposit.js` / `balance.js` / `withdraw.js` | The demo flow: find the wallet, fund it, check it, withdraw from it. |
| `scripts/attack-wrong-user.js` | A different user tries to drain someone else's wallet → the action refuses. |

## Walkthrough

### 1. Install + configure

```bash
cp .env.example .env
npm install
```

Set in `.env`:
- `LIT_API_KEY` — your **account-level (master) API key** from the
  [Chipotle dashboard](https://dashboard.chipotle.litprotocol.com), *not* a
  scoped usage key (setup calls `/add_group`, which rejects scoped keys).
- `RPC_URL` — a Base-Sepolia RPC URL (`https://sepolia.base.org` works). The
  action pins this host (`ALLOWED_RPC_HOST`); to use Alchemy/Infura, edit that
  regex too (which changes the derived wallet addresses).
- `DEPLOYER_PRIVATE_KEY` — an EOA with Base-Sepolia gas. It deploys the token
  and funds the demo wallets. Keep this one private — it is *not* a demo user.

> The "users" are the well-known **public** Hardhat/Anvil test keys in
> `scripts/_users.js`. They are printed in every local-node log on earth — never
> put real funds on them. In a real app you never hold a user's key; you only
> need their **address** to stamp into the action.

### 2. Run setup

```bash
npm run setup
```

Three steps: create a wildcard permission group (each user's action has a
different CID, so we allowlist by group rather than enumerate CIDs), mint a
scoped usage key, deploy `DemoToken`.

### 3. Walk the demo

```bash
# Each user has their OWN wallet — same template, different address baked in:
npm run address -- 0
npm run address -- 1          # different address => different wallet

# Fund user 0's wallet with 100 ABD + a little gas:
npm run deposit -- 0 100
npm run balance -- 0

# User 0 authorizes a withdrawal (signs with their EOA; the action verifies):
npm run withdraw -- 0 0x000000000000000000000000000000000000dEaD 25
npm run balance -- 0

# Attack: user 1 tries to drain user 0's wallet. The action refuses, because
# the signature doesn't recover to user 0's hardcoded address.
npm run attack:wrong-user -- 0 1
```

## Production notes

- **You never need the user's private key.** The demo signs on the user's behalf
  for convenience. In production the user signs the canonical message in their
  own wallet (e.g. `personal_sign` / `eth_signTypedData`); your client just
  relays `{ message-fields, signature }` to the action.
- **Stamp the address server-side.** Build the action source from the template
  the same way `scripts/_users.js` does, using the address from your auth system
  — don't trust a client-supplied `OWNER_ADDRESS`.
- **Consider EIP-712 typed data** instead of the newline string here for a
  nicer wallet signing UX; keep the action's reconstruction identical.
- **The wallet pays its own gas.** It's an EOA under the hood, so fund a little
  native gas alongside the ERC-20 (as `deposit.js` does), or add a relayer/
  paymaster if you want gasless withdrawals.
- **The RPC is a trust input — pin it.** Recipient and amount are signature-bound
  and can never be redirected by the RPC. But the action reads nonce + gas price
  from the caller's `rpcUrl`, so a hostile node could still grief: a bogus gas
  price burns the wallet's native gas on broadcast, and a future nonce lets the
  signed tx execute later than intended. This example pins the RPC host
  (`ALLOWED_RPC_HOST`) and caps the gas price (`MAX_GAS_PRICE_WEI`) to close
  both; harden further by binding fee bounds into the signed authorization.
- **Verify the action source/CID before depositing.** `address.js` runs whatever
  source the client builds, and the group allows any CID (`["0"]`). A malicious
  *client* could therefore show a depositor a wallet derived from a different
  action than advertised — it can't touch funds already in the real CID's wallet,
  but it can misdirect a fresh deposit. Before funding, confirm the exact action
  source (and resulting CID via `/get_lit_action_ipfs_id`) you expect.
- **`DemoToken` is a faucet token** and is unaudited — for demos only.
