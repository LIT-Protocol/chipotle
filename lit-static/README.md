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
| `attestation.js` | TEE attestation verifier for the API origin (AMD SEV-SNP via `/attestation/v3`); the Core SDK runs it before the first request |

No build step — files are served as-is.

## npm distribution and releases

The Core SDK is packaged as `@lit-protocol/core-sdk`. Its package manifest and
consumer documentation live in `sdk/`; the five runtime files remain here as the
single source for both the dashboard and npm, with hand-written TypeScript
declarations (`core_sdk.d.ts`, `attestation.d.ts`) beside the two entry points.
Packaging copies only those files, the consumer README, and the manifest to a
temporary directory. No dashboard,
scripts, tests, or repository documentation is shipped.

Run `node --test lit-static/sdk/scripts/release.test.mjs` and
`node lit-static/sdk/scripts/release.mjs` to validate version selection and install
and exercise the real tarball. Neither command publishes.

When you add or rename an export or a public client method, update the
declarations too. `node --test lit-static/sdk/scripts/types.test.mjs` fails
when the JavaScript and the declarations name different things, and
`LIT_SDK_TYPECHECK=5.6.3 node lit-static/sdk/scripts/release.mjs` compiles a
strict TypeScript consumer (`sdk/scripts/consumer.mts`) against the tarball.
CI runs both on pull requests.

`.github/workflows/core-sdk.yml` validates PRs and publishes changed package
contents after merges to private `main`. Unrelated static asset changes are a
no-op. The first release is 0.1.0; subsequent releases automatically increment
the registry's highest stable patch version. Set a higher version in
`sdk/package.json` for an intentional minor/major release. That checked-in version
is a minimum, not the last published version; CI does not push version commits.
The registry manifest records a content digest for retry detection. Reverting to
older contents creates a new patch release. Publishing jobs serialize and check
out current main after acquiring the lock, so stale queued jobs cannot publish
older code. Registry errors other than package-not-found fail closed. A failed
publish can be retried with workflow_dispatch on main; after an ambiguous network
failure, wait for the registry to expose the release before retrying.

Publishing is off until the repository variable `CORE_SDK_PUBLISH_ENABLED` is
`true`. The SDK's default API URL (`LIT_API_URL` in `attestation.js`) must
resolve and pass attestation before the first release, so turn it on only once
that hostname is live in production.

### One-time npm setup

1. Ensure your npm account can create `@lit-protocol/core-sdk` in the Lit scope.
2. For the first release, set the **private repository** Actions secret
   `CORE_SDK_NPM_TOKEN` to a granular npm token authorized to create/publish this
   package, with bypass 2FA enabled. Never commit the token.
3. After the first publication, configure the package's npm trusted publisher:
   GitHub organization `LIT-Protocol`, repository `chipotle-private`, workflow
   `core-sdk.yml` (no environment). Then remove `CORE_SDK_NPM_TOKEN`; subsequent
   releases authenticate via GitHub OIDC with npm 11.

The workflow publishes the npm package publicly from the private repository, as
intended for this SDK. It never pushes to the public GitHub repository. It does
not request provenance, which npm does not support for private source repos.
See https://docs.npmjs.com/trusted-publishers/ for trusted publisher setup.
