# Lit Triggers examples

Full, runnable examples where a [Lit Triggers](https://triggers.litprotocol.com)
trigger — a webhook, a cron schedule, or an EVM chain event — drives a Lit
Action that fetches data, signs, and transacts with a key no human holds.

These differ from the examples in the [parent folder](../), which call the Lit
Action endpoint directly. Everything here additionally depends on the
lit-triggers service: `setup.js` authorizes your machine in the browser and
creates the trigger. See the [Lit Triggers docs](https://github.com/LIT-Protocol/chipotle/tree/main/docs/lit-triggers)
for concepts and the API.

| Example | Trigger | What it shows |
| --- | --- | --- |
| [`release-attestation`](./release-attestation) | webhook | Verify a GitHub release webhook (HMAC over the raw body) and anchor the release on-chain via a keyless signer. |
| [`uptime-insurance`](./uptime-insurance) | schedule | Parametric insurance: an autonomous ETH payout from a pool key nobody holds when a monitored service is down. |
| [`chainlink-feed-mirror`](./chainlink-feed-mirror) | chain_event | Relay a Chainlink price feed to a chain Chainlink doesn't support, with no trusted relayer. |

Each ships a hardened action, a one-shot `setup` script (action CID, scoped
key, contract deploy, trigger creation), a `deploy` script, and an end-to-end
client.

> Want an agent to wire one up for you? Point it at
> [`https://triggers.litprotocol.com/SKILL.md`](https://triggers.litprotocol.com/SKILL.md).
