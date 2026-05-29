# Examples

Full, runnable examples that show Lit Actions wired up to the surrounding
pieces — smart contracts, deploy scripts, off-chain clients — that the
single-snippet examples in [`docs/lit-actions/examples.mdx`](../docs/lit-actions/examples.mdx)
don't cover.

| Example | What it shows |
| --- | --- |
| [`compliance-transfer-gate`](./compliance-transfer-gate) | An ERC-20 whose every transfer is gated on the Chainalysis on-chain sanctions oracle. The action reads the oracle on Ethereum mainnet and signs an authorization the token contract — on any chain — verifies. Keyless. |
| [`cross-chain-token`](./cross-chain-token) | Burn/mint cross-chain ERC-20. The action observes burn events on one chain (Base Sepolia) and signs the matching mint on another (Arbitrum Sepolia) — permissionless bridging, any chain. Keyless. |
| [`multi-source-price-oracle`](./multi-source-price-oracle) | Spot-price oracle. Fetches from Coinbase, Kraken, and Bitstamp in parallel, takes the median, rejects if the spread is too wide, signs the result for any EVM chain. Keyless. |
| [`prediction-market-oracle`](./prediction-market-oracle) | AI consensus for prediction-market resolution. Polls Perplexity (required, web-grounded) plus OpenAI and Anthropic (optional second opinions); only signs when every configured model agrees. |
| [`private-stablecoin`](./private-stablecoin) | A compliant private stablecoin. Balances and transfers are hidden on-chain (commitment/nullifier notes + encrypted blobs); the Lit Action is the prover that replaces a ZK circuit, with OFAC baked in, KYC at the dollar edge, provable reserves, and warrant-gated selective disclosure. |

If you're looking for a one-file recipe (sign a message, decrypt a secret,
fetch a price and sign it), start with the docs page. Examples here are for
flows that need more than one file to actually run.
