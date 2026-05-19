# Examples

Full, runnable examples that show Lit Actions wired up to the surrounding
pieces — smart contracts, deploy scripts, off-chain clients — that the
single-snippet examples in [`docs/lit-actions/examples.mdx`](../docs/lit-actions/examples.mdx)
don't cover.

| Example | What it shows |
| --- | --- |
| [`compliance-transfer-gate`](./compliance-transfer-gate) | An ERC-20 whose every transfer is gated on a live recipient-risk API. Includes the action, the contract, deploy scripts, and an end-to-end runner. |
| [`multi-rpc-consensus-oracle`](./multi-rpc-consensus-oracle) | Reads an EVM view function from three RPC providers (Infura, Alchemy, QuickNode) in parallel and only signs when all three agree. URLs are encrypted to the PKP and host-whitelisted inside the action. |

If you're looking for a one-file recipe (sign a message, decrypt a secret,
fetch a price and sign it), start with the docs page. Examples here are for
flows that need more than one file to actually run.
