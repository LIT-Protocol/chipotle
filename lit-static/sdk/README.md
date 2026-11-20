# Lit Core SDK

JavaScript client for the Lit Protocol management API. It verifies that the API
it talks to is the governed Lit workload inside a genuine confidential VM
before it sends anything.

```sh
npm install @lit-protocol/core-sdk
```

```js
import { createClient, LIT_API_URL } from '@lit-protocol/core-sdk';

const client = createClient(LIT_API_URL);
const result = await client.accountExists(process.env.LIT_ACCOUNT_API_KEY);
```

See the [API documentation](https://developer.litprotocol.com/management/api_direct)
for account creation, usage keys, wallets, groups, and action execution.
Keep account keys on your server. Action execution requires a usage key.

ES modules; Node.js 22+ or a modern browser with a bundler. The HTTP API and
attestation have no dependencies.

## Attestation

`LIT_API_URL` (`https://api2.chipotle.litprotocol.com`) runs in an AMD SEV-SNP
confidential VM. A client created for it verifies the VM before its first
request. If verification fails, the request rejects with an `AttestationError`
and nothing is sent.

Only that first request waits. After ten minutes the client verifies again in
the background while requests carry on, so verification adds no latency to
them. If a background verification fails, the next request verifies first and
fails if it cannot. A client idle for over an hour verifies before its next
request.

Create one client per process and reuse it. Verification belongs to the client
object, so a client made per request verifies on every request. A verification
takes one to two seconds: the VM's hardware produces one attestation at a
time, and the server queues requests for it, so many clients starting together
wait in line. A client that already holds an attestation is not held up by
that: if a background verification finds the server too busy to answer, the
client keeps the attestation it has, for at most `maxAgeMs`, and tries again
on the next request.

Verification proves, using only AMD's root key and the Base blockchain as
trust anchors:

- the attestation report is signed by a genuine AMD chip, for this request;
- the VM booted the dstack OS image it claims, with debugging disabled;
- the VM runs the Lit app, with a docker-compose document and OS image that
  Lit's multisig approved on-chain;
- in Node.js, that the TLS key of the connection belongs to that same VM. The
  client then pins every API connection to that key, so a request can only
  reach a VM that was verified.

During a release two VMs serve the hostname for a while, the outgoing one and
its replacement, each with its own TLS key. The client verifies whichever one a
connection reaches and pins the keys of both, so requests carry on through the
changeover. A key stays pinned for at most `maxAgeMs` after it was verified. The
first request to reach the new VM waits for that verification, once per client.

```js
const client = createClient(LIT_API_URL);
const proof = await client.verifyAttestation();
// { platform: 'sev-snp', appId, composeHash, osImageHash, tcbStatus, tlsBinding, ... }
```

Options, passed as `createClient({ baseUrl, attestation: { ... } })`:

| Option | Default | Meaning |
| --- | --- | --- |
| `rpcUrls` | public Base endpoints | Base JSON-RPC endpoints for the on-chain check. Use your own for stronger guarantees. |
| `tlsBinding` | `'require'` for Lit origins | `'require'`: in Node.js the TLS certificate must be bound to the VM; a browser cannot check and proceeds. `'strict'`: must be bound everywhere, so it fails in browsers. `'auto'`: checked when possible. `'off'`: skipped. |
| `requireUpToDateTcb` | `false` | Reject a platform whose firmware versions are not all current. Lit's on-chain policy already makes the key service refuse an out-of-date box at boot; this repeats the check on every verification and fails closed during a platform firmware update. |
| `minTcb` | none | Minimum firmware versions, e.g. `{ snp: 24, microcode: 222 }`. |
| `ttlMs` | `600000` | Age after which a request starts a background re-verification. |
| `maxAgeMs` | `3600000` | Age after which a request waits for re-verification. |
| `onVerified` | none | Called with each verification result. |

`attestation: false` turns verification off. For an origin the SDK does not
know (a self-hosted deployment), pass `appId` and `kmsContract` to verify it.

To verify without a client:

```js
import { verifyAttestation } from '@lit-protocol/core-sdk/attestation';
await verifyAttestation('https://api2.chipotle.litprotocol.com');
```

Browsers cannot inspect TLS certificates, so `tlsBinding` reports
`'unsupported'` there and everything else is still verified. Verify from a
server when the channel matters.

## Migrating from `api.chipotle.litprotocol.com`

The original origin runs on Intel TDX and is being retired. It keeps working
until its announced sunset date, and this SDK warns once when a client is
created for it. To migrate, change `baseUrl` to `LIT_API_URL`. Accounts, API
keys, wallets and their addresses are the same on both origins: both run the
same app with the same keys.

If you verify the old origin's TDX attestation yourself, that check does not
apply to the new origin. Use `verifyAttestation` from this package instead.

## ChainSecured mode

ChainSecured mode uses ethers v6: browsers can load it from the SDK's CDN
fallback; Node applications must supply it before using that mode:

```js
import { ethers } from 'ethers';
globalThis.ethers = ethers;
```

Install ethers separately if using ChainSecured mode in Node.
