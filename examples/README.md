# Examples

Full, runnable examples that show Lit Actions wired up to the surrounding
pieces — smart contracts, deploy scripts, off-chain clients — that the
single-snippet examples in [`docs/lit-actions/examples.mdx`](../docs/lit-actions/examples.mdx)
don't cover.

## Lit Actions (direct call)

These call the Lit Action endpoint directly — a caller submits the request and
gets back a signed result.

| Example | What it shows |
| --- | --- |
| [`compliance-transfer-gate`](./compliance-transfer-gate) | An ERC-20 whose every transfer is gated on the Chainalysis on-chain sanctions oracle. The action reads the oracle on Ethereum mainnet and signs an authorization the token contract — on any chain — verifies. Keyless. |
| [`cross-chain-token`](./cross-chain-token) | Burn/mint cross-chain ERC-20. The action observes burn events on one chain (Base Sepolia) and signs the matching mint on another (Arbitrum Sepolia) — permissionless bridging, any chain. Keyless. |
| [`multi-source-price-oracle`](./multi-source-price-oracle) | Spot-price oracle. Fetches from Coinbase, Kraken, and Bitstamp in parallel, takes the median, rejects if the spread is too wide, signs the result for any EVM chain. Keyless. |
| [`prediction-market-oracle`](./prediction-market-oracle) | AI consensus for prediction-market resolution. Polls Perplexity (required, web-grounded) plus OpenAI and Anthropic (optional second opinions); only signs when every configured model agrees. |

## Lit Triggers (event-driven)

These add the [Lit Triggers](https://triggers.litprotocol.com) service: a
webhook, cron schedule, or EVM chain event fires the action automatically —
no caller. See [`lit-triggers/`](./lit-triggers).

| Example | Trigger | What it shows |
| --- | --- | --- |
| [`lit-triggers/release-attestation`](./lit-triggers/release-attestation) | webhook | Verify a GitHub release webhook (HMAC over the raw body) and anchor the release on-chain via a keyless signer. |
| [`lit-triggers/uptime-insurance`](./lit-triggers/uptime-insurance) | schedule | Parametric insurance: an autonomous ETH payout from a keyless pool when a monitored service is down. |
| [`lit-triggers/chainlink-feed-mirror`](./lit-triggers/chainlink-feed-mirror) | chain_event | Relay a Chainlink price feed to a chain Chainlink doesn't support, with no trusted relayer. |

If you're looking for a one-file recipe (sign a message, decrypt a secret,
fetch a price and sign it), start with the docs page. Examples here are for
flows that need more than one file to actually run.
