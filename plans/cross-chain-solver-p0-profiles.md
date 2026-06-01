# Cross-Chain Solver P0 Profiles

Status: draft v0.2
Date: 2026-06-01

These are the first deep-dive profiles for the highest-priority outreach targets from `cross-chain-solver-report.md`. Each profile answers the open questions from v0.1: onboarding model, inventory exposure, latency clues, order format / ERC-7683 fit, public docs vs private partner program, best Lit integration point, and likely public contact channels.

## Summary table

| Target | Onboarding | Hot inventory exposure | Public solver docs | Latency clues | ERC-7683 / order format | Best Lit wedge | Contact channels |
|---|---|---|---|---|---|---|---|
| Across | Permissionless relayer operation; open-source relayer | Yes: relayers front destination capital | Strong public docs and relayer repo | Docs cite ~2s fills; settlement/repayment later via bundles | Explicit ERC-7683 support | Policy-gated relayer inventory and deposit reconstruction | `sales@across.to`, Telegram, GitHub, X |
| deBridge DLN | Docs describe open solver market; production ops details less public | Yes: solvers maintain reserve assets and fill destination | Public protocol/order docs; no full runbook found | Orders can fill within seconds; quote tx should be submitted quickly | DLN-specific order structs/API; no ERC-7683 found | Lit-gated fill/claim signing and reserve-asset controls | Discord, X, GitHub examples |
| Wormhole Settlement / Mayan | Curated/case-by-case solver/driver onboarding | Yes for Swift/MCTP drivers | Public architecture; driver onboarding via support | Mayan auction is 3 seconds; MCTP slower due to finality/CCTP | Mayan/Wormhole-specific formats; no ERC-7683 found | Driver bidding/fill/unlock signing; VAA/CCTP verification | Mayan support/Discord, Wormhole Discord/X |
| 1inch Fusion+ | Resolver onboarding through Business Portal/terms; off-chain auction access reviewed | Yes: resolvers cover gas, deploy escrows, provide liquidity | Public docs, SDKs, examples; production access gated | Dutch auction + escrow/finality/secret reveal windows | 1inch cross-chain SDK / hashlock escrow format; no ERC-7683 found | Resolver escrow signing and maker-side secret custody/reveal | Business Portal, `support@1inch.com`, GitHub |
| Squid Coral / Squid Intents | Integrator enablement required; solver side appears partner/private | Likely yes on solver side; integrator receives signed solver quotes/calldata | Public integrator docs; no public solver runbook found | Claims sub-5s execution; examples poll status every 5s | Squid route/quoteId/request types; no ERC-7683 found | Solver quote signing / inventory policy; integrator execution policy | Discord, `support@squidrouter.com`, public docs contacts |
| Relay.link | Public solver guide; production oracle/signer onboarding coordinated with Relay | Yes: solvers front capital from pre-positioned liquidity | Public solver/oracle docs | Real-time settlement; no batching window | Relay Settlement Protocol / Depository / Hub / Oracle; no ERC-7683 found | Solver fill signer, oracle signer, withdrawal proof signing | `support@relay.link`, GitHub, Relay forms |

## 1. Across / Risk Labs

### How it works

Across is a fast-fill cross-chain intents protocol. Users create deposits/intents on an origin chain. Relayers monitor deposits, front destination-chain liquidity, and later get repaid through Across settlement bundles. Across also supports exclusive relayer fields (`exclusiveRelayer`, `exclusivityDeadline`) for temporarily nominated relayers before the fill becomes open.

### Open question answers

- **Onboarding:** Across docs describe relayer operation as permissionless and Risk Labs maintains open-source relayer software.
- **Inventory exposure:** yes. Relayers front their own destination-chain capital and need balances/gas across supported chains.
- **Latency:** docs position Across around fast, ~2-second fills. Relayers need low-seconds detection, policy, signing, and transaction submission. Repayment/settlement happens later through bundles, so inventory is exposed until repayment.
- **Order format / standards:** Across explicitly supports ERC-7683 cross-chain intents, alongside direct deposit and API routes.
- **Docs:** strong public relayer/protocol docs and public relayer repo.
- **Best Lit integration:** Lit-managed relayer signing, policy-gated destination `fillV3Relay()` execution, exclusive-relayer signing, and post-fill claim/settlement operations. This maps directly to Chipotle's existing Across policy demo.
- **Likely contacts:** `sales@across.to`, Across Telegram, GitHub org/issues, Across X.

### Lit-specific thesis

Across is the cleanest first proof point because the repo already demonstrates an Across-style policy. Lit can reconstruct the source deposit/order, verify recipient/amount/token/chain, enforce caps and kill switches, and only then sign a fill authorization. The relayer bot never holds the inventory-moving key.

### Evidence

- Across cross-chain intents: https://docs.across.to/guides/concepts/crosschain-intents
- Across intents architecture: https://docs.across.to/guides/concepts/intents-architecture
- Across intent lifecycle: https://docs.across.to/guides/concepts/intent-lifecycle
- Across actors/relayers: https://docs.across.to/introduction/actors
- Across ERC-7683: https://docs.across.to/guides/concepts/erc-7683
- Across relayer repo: https://github.com/across-protocol/relayer-v3
- Chipotle Across policy: `examples/lit-solver-vault/action/acrossPolicy.js`

## 2. deBridge DLN

### How it works

deBridge DLN is a 0-TVL cross-chain order network. A user creates a source-chain order describing what they give and what they expect on the destination chain. Solvers/takers detect profitable orders, fulfill the destination leg with their own funds/reserve assets, then claim or unlock the source-side funds through DLN's settlement/messaging flow.

### Open question answers

- **Onboarding:** docs describe DLN as an open market where anyone can become a solver. However, a full production solver runbook was not found in public docs, so practical production onboarding should be verified.
- **Inventory exposure:** yes. Solvers maintain reserve assets and fill destination-side orders before claiming source-side value.
- **Latency:** profitable market orders are expected to be filled within seconds; quoted transactions should be submitted quickly. Solver latency likely needs to be seconds-level.
- **Order format / standards:** DLN uses its own order structures and APIs; no ERC-7683 compatibility was found in primary docs.
- **Docs:** public protocol/order fulfillment docs; production solver operations details appear less complete.
- **Best Lit integration:** destination fulfillment signing, source claim/unlock signing, reserve-asset inventory policy, route/profitability checks, and per-chain hot-key isolation.
- **Likely contacts:** deBridge Discord, X, GitHub examples, public founder/CEO X.

### Lit-specific thesis

DLN solvers are exactly the kind of operators who need fast signing but should not expose reserve-asset keys directly to bots. Lit can gate both the fill and claim side with DLN order reconstruction, profitability checks, route limits, and reserve-asset thresholds.

### Evidence

- DLN introduction: https://docs.debridge.com/dln-details/overview/introduction
- Protocol overview: https://docs.debridge.com/dln-details/overview/protocol-overview
- Order fulfillment: https://docs.debridge.com/dln-details/dln-specifics/order-fulfillment/order-fulfillment
- Fulfilling order: https://docs.debridge.com/dln-details/dln-specifics/order-fulfillment/fulfilling-order
- Claiming order: https://docs.debridge.com/dln-details/dln-specifics/order-fulfillment/claiming-order
- Reserve assets: https://docs.debridge.com/dln-details/dln-specifics/reserve-assets
- API integrator example: https://github.com/debridge-finance/api-integrator-example

## 3. Wormhole Settlement / Mayan / MCTP

### How it works

Wormhole Settlement is an intent-oriented settlement product. Mayan Swift runs a short auction among drivers/solvers, then a winning driver fulfills the destination leg and later unlocks source-side value. Mayan MCTP uses CCTP for USDC movement and Wormhole messages/attestations for coordination. Wormhole core also supplies guardian attestations and executor/relayer infrastructure.

### Open question answers

- **Onboarding:** Wormhole/Mayan solver driver onboarding appears curated or case-by-case via Mayan support/Discord. Wormhole relayers for message transfer are a separate, more permissionless category.
- **Inventory exposure:** yes for Swift/MCTP drivers. Drivers bid, fill, redeem, unlock, and/or swap with operational liquidity and signing keys.
- **Latency:** Mayan auction is 3 seconds, so bidding and fill authorization are latency-sensitive. MCTP is slower because of chain finality and CCTP constraints.
- **Order format / standards:** Mayan/Wormhole-specific intent and message formats; no ERC-7683 compatibility found.
- **Docs:** public architecture and SDK docs; driver onboarding is not fully self-serve.
- **Best Lit integration:** driver bidding/fill signer, source unlock signer, MCTP redeemer/unlocker keys, Wormhole VAA/CCTP verification policy, and executor provider keys.
- **Likely contacts:** Mayan `support@mayan.finance` / Discord; Wormhole Discord/X and docs channels.

### Lit-specific thesis

The 3-second auction means Lit's value must be framed carefully: not every check belongs in the hot bidding path. The best design may separate (1) fast pre-approved bidding constraints, (2) destination fill signing, and (3) slower redeem/unlock/claim automation. Lit can also verify Wormhole/CCTP attestations before downstream signing.

### Evidence

- Wormhole Settlement overview: https://wormhole.com/docs/products/settlement/overview/
- Wormhole Settlement architecture: https://wormhole.com/docs/products/settlement/concepts/architecture/
- Wormhole Settlement FAQ: https://wormhole.com/docs/products/settlement/faqs/
- Wormhole Settlement get started: https://wormhole.com/docs/products/settlement/get-started/
- Wormhole Executor framework: https://wormhole.com/docs/protocol/infrastructure/relayers/executor-framework/
- Mayan auction: https://docs.mayan.finance/architecture/auction
- Mayan MCTP: https://docs.mayan.finance/architecture/mctp
- Mayan relayers: https://docs.mayan.finance/architecture/relayers

## 4. 1inch Fusion+

### How it works

1inch Fusion+ extends the Fusion resolver model to cross-chain swaps. Users/makers sign intent-style orders. Resolvers compete in a Dutch auction, deploy source/destination escrows, provide taker-side liquidity, cover gas, and use hashlock/timelock mechanics plus secret reveal to complete settlement.

### Open question answers

- **Onboarding:** resolvers onboard through the 1inch Business Portal, accept resolver terms, provide details/contracts, and may need technical review for off-chain auction access. This is not fully permissionless production onboarding.
- **Inventory exposure:** yes. Resolvers cover gas, deploy escrows, and provide liquidity/fills.
- **Latency:** no exact public SLA found. Latency-sensitive points include quote/auction response, escrow deployment, secret reveal readiness, and cancellation windows.
- **Order format / standards:** 1inch cross-chain SDK and hashlock escrow model; no ERC-7683 found.
- **Docs:** public resolver docs, SDK, and examples; production access is gated by portal/support.
- **Best Lit integration:** resolver-side escrow/fill/cancel signing and maker-side secret custody/reveal. Maker-side secret custody may be uniquely strong because Fusion+ relies on correct secret reveal timing.
- **Likely contacts:** 1inch Business Portal, `support@1inch.com`, GitHub examples.

### Lit-specific thesis

Fusion+ has two Lit wedges: resolver key protection and maker secret custody. A Lit Action can reveal secrets only after escrow/finality conditions are verified, and can sign resolver transactions only if auction, route, and profitability policy pass.

### Evidence

- Fusion+ intro: https://business.1inch.com/portal/documentation/apis/swap/fusion-plus/introduction
- Fusion intro: https://portal.1inch.dev/documentation/apis/swap/fusion/introduction
- Resolver onboarding: https://business.1inch.com/portal/assets/docs-v2/resolvers/introduction.md
- Cross-chain SDK: https://github.com/1inch/cross-chain-sdk
- Cross-chain resolver example: https://github.com/1inch/cross-chain-resolver-example
- Fusion resolver example: https://github.com/1inch/fusion-resolver-example

## 5. Squid Coral / Squid Intents

### How it works

Squid is a cross-chain router built around Axelar messaging and liquidity integrations. Squid Intents / Coral adds signed solver quote flows for fast, exact execution. Public docs are primarily for integrators receiving route/quote/calldata responses.

### Open question answers

- **Onboarding:** integrators need Squid enablement / integrator IDs for intents. Public solver onboarding docs were not found, so solver participation appears private/partner or internal.
- **Inventory exposure:** likely yes on the solver side because signed solver quotes imply economic backing and fast execution. Integrators mostly consume quotes/calldata.
- **Latency:** Squid markets sub-5-second execution; examples poll status every 5 seconds. Route generation may be rate-limited because it includes signed solver quotes.
- **Order format / standards:** Squid route, `quoteId`, and transaction request types; no ERC-7683 found.
- **Docs:** public integrator docs; solver runbook appears private.
- **Best Lit integration:** solver quote signing, inventory-release policy, route/quote constraints, or integrator-side execution gating.
- **Likely contacts:** Squid Discord, `support@squidrouter.com`, public docs contacts including `nick@squidrouter.com` / Telegram.

### Lit-specific thesis

Squid is a strong partner/integrator target if the solver side is private. The report should ask them to verify whether Coral solvers hold inventory, where signed solver quotes are produced, and whether Lit could gate quote signing or route execution.

### Evidence

- Squid docs: https://docs.squidrouter.com/
- Squid overview: https://docs.squidrouter.com/getting-started/readme
- Squid Intents: https://docs.squidrouter.com/api-and-sdk-integration/key-concepts/squid-aggregator/squid-intents
- Coral Intent Swaps: https://docs.squidrouter.com/api-and-sdk-integration/coral-intent-swaps
- Integrating Squid Intents: https://docs.squidrouter.com/api-and-sdk-integration/coral-intent-swaps/integrating-squid-intents
- Status API: https://docs.squidrouter.com/api-and-sdk-integration/api/status
- Examples: https://github.com/0xsquid/examples

## 6. Relay.link

### How it works

Relay exposes fast bridge/execution APIs and a settlement protocol. Solvers front destination-chain actions from pre-positioned liquidity, while settlement and accounting happen through Relay protocol components such as Depository, Oracle, Relay Chain, Hub, and Allocator. Relay also documents third-party oracle signer operation.

### Open question answers

- **Onboarding:** public solver docs exist. Production oracle/signer participation requires coordination with Relay before expecting signatures to be used downstream, so production participation is coordinated rather than clearly permissionless.
- **Inventory exposure:** yes. Solvers front capital and/or destination actions from pre-positioned liquidity.
- **Latency:** settlement happens in real time with no batching window; product posture is low-latency user execution. No concrete fill SLA found.
- **Order format / standards:** Relay Settlement Protocol-specific order/deposit and oracle components; no ERC-7683 found.
- **Docs:** public solver and oracle docs are unusually detailed.
- **Best Lit integration:** solver fill signer, oracle signer, withdrawal proof signer, and policy-controlled liquidity withdrawal/rebalancing. Relay's oracle docs mention signer modes such as raw private key or AWS KMS, making Lit a natural signing backend candidate if accepted by Relay.
- **Likely contacts:** `support@relay.link`, GitHub repos, Relay team forms.

### Lit-specific thesis

Relay has one of the clearest signing-backend hooks because its third-party oracle docs already discuss signer modes. Lit can be pitched as a signer backend with richer policy checks than raw keys or standard KMS, especially for solver fill and withdrawal paths.

### Evidence

- Relay docs: https://docs.relay.link/
- Deposit addresses: https://docs.relay.link/features/deposit-addresses
- For solvers: https://docs.relay.link/references/protocol/guides/for-solvers
- Third-party oracle: https://docs.relay.link/references/protocol/guides/third-party-oracle
- Protocol overview: https://docs.relay.link/references/protocol/overview
- Protocol addresses: https://docs.relay.link/references/protocol/addresses
- Relay Kit: https://github.com/relayprotocol/relay-kit
- Relay Periphery: https://github.com/relayprotocol/relay-periphery

## Cross-target answers to v0.1 open questions

### 1. Permissionless vs allowlisted/professional onboarding

- **Most permissionless:** Across relayers; deBridge DLN solvers according to docs.
- **Public docs but coordinated production access:** Relay.link solvers/oracles.
- **Curated / portal / partner access:** Wormhole Settlement / Mayan drivers, 1inch Fusion+ resolvers, Squid Coral solvers.

### 2. Who holds hot inventory?

- **Clearly yes:** Across relayers, deBridge DLN solvers, Wormhole/Mayan drivers, 1inch Fusion+ resolvers, Relay solvers.
- **Likely yes but needs confirmation:** Squid Coral solvers.
- **Integrator-only APIs:** Squid/Relay/LI.FI-style API consumers may only receive calldata, but the underlying solver/relayer still bears custody risk.

### 3. Is 300-500 ms Lit policy authorization plausible?

- **Probably plausible for destination fill authorization:** Across, deBridge, Relay if the bot can start checks immediately and source reads are efficient.
- **Needs careful path design:** Wormhole/Mayan 3-second auctions and Squid sub-5-second execution. Use pre-computed policy, cached allowlists, and minimal hot-path checks.
- **Likely acceptable for slower paths:** claims, withdrawals, unlocks, settlement, rebalancing, oracle signing, and secret reveal after finality.

### 4. ERC-7683 compatibility

- **Clearly yes:** Across.
- **Not found in primary docs:** deBridge DLN, Wormhole/Mayan, 1inch Fusion+, Squid, Relay.
- **Report action:** ask each team whether they support ERC-7683, plan to, or intentionally use a different order format.

### 5. Best Lit integration point

- **Across:** relayer fill signer + destination inventory vault.
- **deBridge DLN:** destination fill signer + source claim/unlock signer.
- **Wormhole/Mayan:** driver bidding/fill/unlock signer + VAA/CCTP verification policy.
- **1inch Fusion+:** resolver escrow signer + maker secret custody/reveal.
- **Squid Coral:** solver quote signer + inventory policy; integrator execution policy if solver access is private.
- **Relay:** solver fill signer + oracle signer + withdrawal/rebalance signer.

### 6. Public docs vs private programs

- **Most public:** Across, deBridge protocol mechanics, Relay solver/oracle docs.
- **Public architecture but private onboarding:** Wormhole/Mayan drivers, 1inch resolver production access, Squid solver side.

### 7. Likely contact strategy

- Start with public support/BD channels and ask for a technical reviewer, not a sales call.
- Attach or link the company-specific profile and the Chipotle solver-vault demo.
- Ask them to correct factual claims first; only then pivot to whether policy-gated signing maps to their operator model.
