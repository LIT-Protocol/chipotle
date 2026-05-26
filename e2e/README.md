# e2e — Playwright tests for the Chipotle dashboard

UI-level coverage for `lit-static/dapps/dashboard`. Exercises the same
functional surface as the k6 suite (chain config, account creation, usage API
keys, Lit Action execution, encrypt/decrypt) but through the dashboard's UI,
plus ChainSecured (wallet) flows that the k6 tests don't cover.

## What's in here

```
e2e/
├── playwright.config.ts        # projects: api-mode, eoa, walletconnect, flows-*
├── package.json
├── tsconfig.json
├── Makefile                    # wraps ../local_test.sh
├── wallet-setup/
│   └── basic.setup.ts          # Synpress wallet cache: imports Anvil seed + adds Anvil net
├── fixtures/
│   ├── anvil.ts                # snapshot/revert + cheat-RPC helpers, viem clients
│   ├── api-client.ts           # direct HTTP client for lit-api-server (/core/v1)
│   ├── contracts.ts            # reads ../lit-api-server/NodeConfig.toml for the proxy address
│   ├── dashboard.ts            # DashboardPage — selectors + helpers over the dapp UI
│   ├── wc-wallet.ts            # headless @reown/walletkit wallet, signs with viem
│   └── test.ts                 # combined fixture: dashboardPage, apiClient, metamask, wcWallet, anvilSnap
└── tests/
    ├── api/                    # API-mode (auth via account API key)
    │   ├── smoke.spec.ts
    │   ├── new-account.spec.ts
    │   ├── lit-action-encrypt-decrypt.spec.ts
    │   ├── lit-action-ecdsa-sign.spec.ts
    │   └── get-ipfs-cid.spec.ts
    ├── eoa/                    # ChainSecured via MetaMask (Synpress)
    │   ├── connect.spec.ts
    │   └── new-chainsecured-account.spec.ts
    ├── walletconnect/          # ChainSecured via headless WC v2 wallet
    │   ├── connect.spec.ts
    │   └── new-chainsecured-account.spec.ts
    └── flows/
        └── onboarding.spec.ts  # parameterized: runs once per walletKind (api/eoa/wc)
```

## Test pyramid

Existing layers:
- `forge test` — contract correctness
- Rust `cargo test` — backend unit tests
- k6 — API correctness/load against `lit-api-server`

This package adds the top of the pyramid: real Chromium, real dashboard UI,
real wallets, real contracts on Anvil, real backend (with the dstack-simulator
mocking TEE attestation). Keep it lean — push everything that *can* live in
the layers below down to them.

## Prerequisites

Everything needed by `local_test.sh`:

- Foundry on PATH (`anvil`, `forge`, `cast`)
- `pnpm` and Node 20+
- A built `dstack-simulator` at `$SIMULATOR_DIR` (default
  `~/GitHub/dstack/sdk/simulator`)
- `static-web-server` (`brew install static-web-server`)
- Docker is optional — `local_test.sh` runs Jaeger for OTLP telemetry when
  Docker is reachable, and skips it otherwise. Nothing in the test suite
  depends on Jaeger being up.
- A WalletConnect project id in `WC_PROJECT_ID` (free at
  https://cloud.reown.com) — only required for the `walletconnect` projects

## Quick start

```bash
make install
export WC_PROJECT_ID=...        # only needed for WC tests
make up                         # boots the full stack via ../local_test.sh
make test                       # runs the suite
make down                       # stops the stack
```

Subsets:

```bash
make test-api      # API-mode only (no Synpress, no WC — fastest)
make test-eoa      # MetaMask via Synpress
make test-wc       # WalletConnect headless wallet
make test-flows    # the parameterized onboarding flow under all three modes
```

If you want to keep the cargo services running in your own terminal (e.g. to
attach a debugger or watch logs), use:

```bash
make up-no-code
# follow the printed commands to start lit-api-server / lit-actions / static-web-server
```

## How the projects map

| Playwright project | Test dir / matcher              | Wallet stack                |
| ------------------ | ------------------------------- | --------------------------- |
| `api-mode`         | `tests/api/*.spec.ts`           | none                        |
| `eoa`              | `tests/eoa/*.spec.ts`           | Synpress (MetaMask)         |
| `walletconnect`    | `tests/walletconnect/*.spec.ts` | headless `@reown/walletkit` |
| `flows-api`        | `tests/flows/api-*.spec.ts`     | none                        |
| `flows-wc`         | `tests/flows/wc-*.spec.ts`      | headless `@reown/walletkit` |

Flow specs split per-file so the API flow doesn't drag in the WalletConnect
fixture (or require `WC_PROJECT_ID`). The EOA onboarding journey lives in
`tests/eoa/new-chainsecured-account.spec.ts` — those specs need Synpress
regardless.

## Adding a new test

1. If it's wallet-specific, drop it in `tests/eoa/` or `tests/walletconnect/`.
2. If it's pure UI / API mode, drop it in `tests/api/`.
3. If it's a user journey, drop it in `tests/flows/` with an `api-` or `wc-`
   prefix so the right project picks it up. Don't try to make a single file
   serve both — destructuring `wcWallet` in an API-flow test forces the WC
   fixture to run even when nothing uses it.
4. Tests get an automatic Anvil snapshot/revert from the `anvilSnap`
   auto-fixture in `fixtures/test.ts`. Chain state is reset between tests
   without per-test cleanup code.

## How WalletConnect pairing works

The dashboard uses `@walletconnect/ethereum-provider`'s built-in QR modal. We
don't scrape that modal — instead, `lit-static/wallet_connect.js` re-emits the
SDK's `display_uri` event as a DOM event:

```js
window.dispatchEvent(new CustomEvent('lit:wc-display-uri', { detail: uri }));
```

`DashboardPage.waitForWcPairingUri()` listens for this and returns the URI;
the test then pairs the headless wallet directly:

```ts
const [uri] = await Promise.all([
  dashboardPage.waitForWcPairingUri(),
  dashboardPage.startWalletLogin('walletconnect'),
]);
await wcWallet.pair(uri);
```

## Gotchas

**Synpress wallet cache.** The cache is keyed by the contents of
`wallet-setup/basic.setup.ts`. Changing it triggers a one-time rebuild. In CI,
cache `node_modules` and the Playwright browsers but NOT `.cache-synpress/` —
stale wallets are worse than re-running the setup.

**WalletConnect session bleed.** The `wcWallet` fixture calls `disconnectAll`
in teardown. If you create wallets manually, do the same — leftover sessions
cause weird state across tests.

**Don't run MetaMask projects fully in parallel.** The `eoa` and `flows-eoa`
projects share a wallet cache. `workers: 2` in CI is usually safe; higher and
you'll see flakiness on extension load. API and WC tests can parallelize
freely once you split the projects.

**TEE in tests.** `local_test.sh` runs the Rust backend with the
dstack-simulator socket, so attestation is mocked. Reserve a separate (slower)
CI job for real-hardware TEE coverage.

**ChainSecured tests use Anvil account #0 by default.** Synpress imports the
Anvil mnemonic — account #0 is also the contract deployer in `local_test.sh`.
That's fine for the current specs but if you start asserting on specific
balances or storage slots, switch the user to a fresh account inside the test
(`metamask.switchAccount(...)`).
