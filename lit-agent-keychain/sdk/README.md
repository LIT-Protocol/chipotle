# @lit-protocol/keychain

Zero-dependency JavaScript SDK for [Lit Keychain](https://keychain.litprotocol.com).
ES modules for Node.js 18+, Deno, Bun, and browser bundlers.

## Install

```sh
npm install @lit-protocol/keychain
```

## Read a secret

Create an agent usage API key in the Lit Keychain dashboard and supply it through
your environment:

```js
import { LitAgentKeychain, LitAgentKeychainError } from '@lit-protocol/keychain';

const keychain = new LitAgentKeychain({
  usageApiKey: process.env.LIT_AGENT_KEYCHAIN_KEY,
});

const openaiKey = await keychain.get('OPENAI_API_KEY');
```

The SDK obtains a policy-approved grant, then redeems it directly with Chipotle.
Plaintext travels from Chipotle to your agent.

The constructor also accepts `baseUrl` (default
`https://keychain.litprotocol.com`), `timeoutMs` (default `30000`), and a custom
`fetch` implementation. Keep agent usage keys out of public client bundles.

- `get(name, { version, signal } = {})`: retrieve a plaintext secret.
- `grant(name, { version, signal } = {})`: obtain a grant without redeeming it.
- `reference(name, { version, signal } = {})`: obtain ciphertext and vault metadata
  for use inside a permitted Lit Action.

`LitAgentKeychain` is also the default export. `LitAgentKeychainError` exposes
`status`, `body`, and `code` (when the control plane supplies a denial code).

## Build and publish

From `lit-agent-keychain`, run:

```sh
./publish.sh
```

This logs into npm, builds the SDK, and publishes `@lit-protocol/keychain` publicly.
Use an npm account with publishing access to the `@lit-protocol` organization.
Additional arguments are forwarded to `npm publish`, for example
`./publish.sh --dry-run` (still logs in).

No dependency installation is needed. The build checks JavaScript syntax and
copies the ESM source into `dist/`. `npm pack` and `npm publish` also build
automatically through `prepack`.

For subsequent releases, update the version first:

```sh
cd sdk
npm version patch --no-git-tag-version
cd ..
./publish.sh
```
