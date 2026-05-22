# lit-payments-contracts

Solidity contracts for the lit-payments service. Currently just
`LitkeyPaymentGateway` — the single-entrypoint contract a user calls to pay
LITKEY for Stripe credit.

## Layout

```
contracts/                              Hardhat project root.
├── contracts/
│   ├── LitkeyPaymentGateway.sol        The contract.
│   └── test/MockERC20.sol              Test-only mintable token.
├── scripts/deploy.ts                   Deploy + (optional) Basescan verify.
├── test/LitkeyPaymentGateway.test.ts   Unit tests.
├── hardhat.config.ts
├── package.json
└── tsconfig.json
```

## Local

```sh
cd lit-payments/contracts
npm install
npm test
```

## Deploy

```sh
cp .env.example .env   # fill in DEPLOYER_PRIVATE_KEY + TREASURY_ADDRESS
npm run deploy:base-sepolia      # testnet first, always
# then, once you're happy:
npm run deploy:base              # mainnet
```

If `BASESCAN_API_KEY` is set in `.env`, the deploy script verifies the
contract automatically after a 10-second indexing delay. Otherwise verify
later:

```sh
npm run verify:base-sepolia -- <deployed_address> <LITKEY_ADDRESS> <TREASURY_ADDRESS>
```

## Constructor arguments

| Arg        | Value                                                              |
| ---------- | ------------------------------------------------------------------ |
| `litkey`   | LITKEY on Base mainnet: `0xf732a566121fa6362e9e0fbdd6d66e5c8c925e49` |
| `treasury` | Company Safe on Base. Set in `.env` as `TREASURY_ADDRESS`.         |

## Base mainnet deployment

The production Base mainnet gateway is deployed at:

```text
0xa2d54cd1D1dF1735718A857aC49CaF9ECaB0093b
```

Deployment notes:

- Chain: Base mainnet (`chainId` 8453).
- LITKEY token: `0xf732a566121fa6362e9e0fbdd6d66e5c8c925e49`.
- Treasury: company Safe on Base.
- Smoke test: Chris sent 1 wei of LITKEY through the gateway and confirmed it
  arrived in the company Safe.

The `litkey` token address and `treasury` recipient address are constructor
arguments stored as immutables. They cannot be changed in place. If either the
accepted token or recipient Safe needs to change, deploy a new
`LitkeyPaymentGateway` and update the lit-payments listener/dashboard config to
point at the new address.

## Design

See `plans/lit-payments-app.md` (under "Feature 2 — LITKEY → Stripe credit").

In short: a user calls `pay(amount, wallet)` after approving the gateway for
`amount` LITKEY. The contract forwards the tokens to `treasury` and emits
`Payment(wallet, payer, amount)`. The off-chain listener watches that event,
maps `wallet` to a Stripe customer via `metadata.wallet_address`, and writes
a balance_transaction for the credit.

The contract is intentionally immutable — no owner, no upgrade path, no
pause switch. If the design changes, deploy a new contract and update the
listener + dashboard to point at the new address. (Old payments to the old
contract still credit correctly because the listener keeps watching it
until the listener config is updated.)
