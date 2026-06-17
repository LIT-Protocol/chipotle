# lit-static

Browser-facing static assets: the dashboard, the JS Core SDK, and contract
ABIs. Served by `static-web-server` (locally on `:8080` via `local_test.sh`;
in production behind the dashboard domain).

| Asset | Purpose |
|-------|---------|
| `core_sdk.js` | JS client for the `/core/v1/` API (`LitNodeSimpleApiClient`) — the SDK referenced in the [API docs](https://developer.litprotocol.com/management/api_direct) |
| `dapps/dashboard/` | The Chipotle dashboard (accounts, usage keys, groups, actions, wallets, action runner, billing) |
| `account_config_full_abi.js` / `account_config_view_abi.js` | AccountConfig contract ABIs (write / read-only) |
| `wallet_connect.js` | WalletConnect helpers for ChainSecured-mode signing |
| `tx_lifecycle.js` | Wait-for-receipt / on-chain verification helpers |

No build step — files are served as-is.
