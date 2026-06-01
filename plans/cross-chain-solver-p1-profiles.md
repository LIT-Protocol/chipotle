# Cross-Chain Solver P1 Profiles

Status: draft v0.3
Date: 2026-06-01

These are follow-up profiles for P1/P2 targets from `cross-chain-solver-report.md`. They extend the P0 profile format to mature solver DEXs, RFQ systems, route/execution layers, and clearing infrastructure. Unknowns are called out explicitly where public docs do not support stronger claims.

## Summary table

| Target | Onboarding | Hot inventory / custody exposure | Public solver docs | Latency clues | ERC-7683 / order format | Best Lit wedge | Likely contacts |
|---|---|---|---|---|---|---|---|
| UniswapX | Permissionless filling; Ethereum mainnet RFQ quoting is vetted by Uniswap Labs | Yes: fillers provide output liquidity or execute through their own strategies | Strong public filler/quoter docs | RFQ quoter response target: 500 ms; order polling up to 6 RPS | UniswapX reactor/order formats with Permit2; no ERC-7683 found | Filler executor signing, RFQ quote/fill policy, inventory approvals | UniswapX support form, Telegram, GitHub |
| CoW Protocol | Open solver competition, but production onboarding includes bonding/KYC/staging/whitelisting | Partial/yes: execution keys, solver buffers/liquidity, settlement tx signing | Strong public solver and auction docs | Settlement deadline is chain-specific blocks after settle request; mainnet 3 blocks | CoW orders + batch auction schemas; no ERC-7683 found | Solver driver/settlement signer, buffer/inventory policy, hook safety checks | CoW Solvers Telegram, Discord, forum, GitHub |
| LI.FI Intents | Docs describe permissionless solving; hosted order server/API flows may require API key/UI | Yes: solvers deliver destination assets with own capital before settlement | Strong public solver docs | Real-time WebSocket order stream; quote examples include 30s expiry; escrow unlock often <2 min | OIF-style `StandardOrder` / `MandateOutput`; no ERC-7683 found | Solver fill signer, quote inventory publishing, settlement/finalise signer | LI.FI partner form, public team emails, GitHub/X |
| Socket / Bungee | Bungee API production access gated; Socket Watchers/Switchboards more permissionless | Bungee solver inventory unknown; Socket transmitter/filler exposure depends on app | Strong integrator/protocol docs; no public Bungee solver runbook found | Bungee deposit address expires after 10 min; backend default 20 RPS | Socket explicitly documents ERC-7683; Bungee API uses own quote/request/status formats | Transmitter/filler/paymaster signer; deposit/refund policy | Bungee API form, Socket Discord/GitHub/X |
| Synapse Intent Network | Relayer role described as permissionless, but current relaying requires authorization; Quoters/Provers permissioned | Yes: relayers provide destination liquidity, then prove/claim source escrow | Strong RFQ/relayer docs | Active RFQ WebSocket; exclusivity often ~30s; claim delayed by optimistic dispute period | FastBridge/RFQ `BridgeTransactionV2`; no ERC-7683 found | Relayer `relay`, `prove`, and `claim` signers with role-specific policies | Synapse support, Telegram, Discord, GitHub/X |
| Router Protocol OGA | Docs describe permissionless node registry; production API keys/team contact likely needed | Mixed: non-custodial router, but node/solver reserves may exist | Strong public OGA/API docs | Real-time quotes; route responses include `executionDuration`; no hard solver SLA found | Xplore route/quote/step transaction formats; no ERC-7683 found | Policy signer for OGA node/solver reserve execution | Router Discord, Telegram, GitHub/X |
| Hashflow | Permissioned/partnered market-maker and taker/API access | Yes: market makers operate pools, liquidity, quote signing keys | Detailed public maker/taker docs; access allowlisted | Price levels every second; maker WS pings every 30s; quote expiry enforced | Hashflow RFQ quote payloads; EIP-191 signatures; no ERC-7683 found | Market-maker quote signing and cross-chain quote policy | Discord, X, governance/forum, website |
| Bebop | API/production support gated; BopAMM closed beta/application-based | Yes: RFQ PMMs quote from own inventory; solvers compete for aggregation | Strong API docs and use-case pages | Short-expiry quotes: 3-5s; speed-optimized quote target <100 ms | Bebop RFQ/JAM/Aggregation formats; EIP-712; no ERC-7683 found | PMM/solver signing, quote expiry and inventory guardrails | Typeforms, Discord, GitHub/X, LinkedIn |
| Enso | API-key based; no public external solver runbook found | Not direct in docs; users/integrators sign generated calldata | Strong routing/simulation docs | Quoter simulation metadata cached 5 min; default API 10 RPS | Enso API tx/route formats; no ERC-7683 found | Lit + Enso Quoter: sign only exact validated transactions | Discord, Telegram, GitHub/X |
| Everclear | Public protocol/API docs; no complete production solver runbook found | Yes for solvers/fillers: destination stored balances and fast-path inventory | Good concepts/API/contracts docs | Fast path/cross-chain swap guides advertise ~1-2 min; normal queues depend on agents | Everclear `Intent` / `newIntent` / `fillIntent`; no ERC-7683 found | Solver `fillIntent` and rebalance/settlement signing | Discord, Telegram, X |

## 1. UniswapX

### How it works

UniswapX is an open-source, auction-based swapping protocol. Swappers sign Permit2-backed orders describing their desired outputs. Fillers monitor open orders and compete to settle them on-chain through UniswapX reactors. Fillers can fill directly from inventory or use custom Executor contracts implementing `IReactorCallback` to source liquidity from arbitrary strategies.

Ethereum mainnet adds an RFQ plus exclusive Dutch auction flow: vetted quoters respond to RFQs, the best quoter receives exclusive fill rights during an exclusivity window, and permissionless fillers can fill after exclusivity expires or by paying an override.

### Open question answers

- **Onboarding:** anyone can fill UniswapX orders. Ethereum mainnet RFQ quoter participation is vetted by Uniswap Labs and intended for experienced DeFi teams.
- **Inventory exposure:** yes. Direct fillers approve and spend output tokens from their own address; custom Executors may route through external liquidity.
- **Latency:** RFQ quoter servers must respond within **500 ms** or risk suspension. Fillers should poll orders up to **6 RPS**. Ethereum RFQ traffic is estimated around **1 RPS** in docs.
- **Order format / standards:** UniswapX uses reactor/order architecture and Permit2 witness transfers. No ERC-7683 support found in primary docs.
- **Docs:** strong public filler/quoter/contract docs; mainnet quoter access is a vetted program.
- **Best Lit integration:** filler Executor/direct-fill signer. Lit can enforce order reconstruction, reactor allowlists, token/amount bounds, price/profit checks, per-chain spend caps, and exclusivity controls before signing fills or approvals.
- **Likely contacts:** UniswapX support form, Telegram `t.me/UniswapXdiscussion`, Uniswap GitHub.

### Lit-specific thesis

UniswapX has a clear hot-path signing problem: fillers need low-latency execution while guarding token approvals and inventory. For RFQ quoters, 500 ms quote latency means heavy checks should be precomputed, but Lit can still gate fill execution and inventory release after quote selection.

### Evidence

- UniswapX overview: https://developers.uniswap.org/docs/liquidity/uniswapx/overview
- Architecture / reactors: https://developers.uniswap.org/docs/liquidity/uniswapx/concepts/architecture
- Fillers overview: https://developers.uniswap.org/docs/liquidity/uniswapx/filling/overview
- Become a quoter: https://developers.uniswap.org/docs/liquidity/uniswapx/filling/mainnet/become-a-quoter
- Filling on mainnet: https://developers.uniswap.org/docs/liquidity/uniswapx/filling/mainnet/filling-on-mainnet
- Filler FAQ: https://developers.uniswap.org/docs/liquidity/uniswapx/filling/faq
- UniswapX repo: https://github.com/Uniswap/UniswapX

## 2. CoW Protocol

### How it works

CoW Protocol collects user intents off-chain and groups them into batch auctions. Solvers submit solutions for each auction, selecting which orders to execute and how to route them. The protocol selects winning solutions according to surplus and fairness rules, and settlement executes on-chain through CoW settlement contracts. A driver component can sit between CoW autopilot and a solver engine, handling preprocessing, liquidity fetching, sanity checks, and transaction encoding.

### Open question answers

- **Onboarding:** open solver competition, but production onboarding is structured: shadow competition, KYC if joining the CoW DAO bonding pool, staging, production, bonding/whitelisting, and possibly further evaluation for mainnet.
- **Inventory exposure:** yes/partial. Solvers need settlement/execution signing keys, may access DEX liquidity, and may use internal buffers/funds in the settlement contract. They do not custody user funds directly.
- **Latency:** settlements must execute before the auction deadline. Public docs list network-specific deadlines set as blocks after settle request, including mainnet **3 blocks** and larger L2 windows.
- **Order format / standards:** CoW uses its own intent parameters and batch-auction JSON schemas. No ERC-7683 support found in primary docs.
- **Docs:** strong public solver, API, driver, onboarding, and competition docs; production participation still involves bonding/KYC/whitelisting.
- **Best Lit integration:** solver settlement signer and driver policy layer. Lit can enforce settlement deadlines, route allowlists, buffer usage limits, hook safety, simulation requirements, and solver risk limits before signing.
- **Likely contacts:** CoW Swap Solvers Telegram group, CoW Discord, forum, GitHub.

### Lit-specific thesis

CoW is less about guarding cross-chain inventory and more about guarding a high-value solver execution stack. Lit can sit at the signer boundary for settlement transactions, ensuring driver-generated calldata satisfies CoW auction rules, solver risk limits, and hook/buffer policies.

### Evidence

- Solvers overview: https://docs.cow.fi/cow-protocol/concepts/introduction/solvers
- Solver onboarding: https://docs.cow.fi/cow-protocol/tutorials/solvers/onboard
- Driver docs: https://docs.cow.fi/cow-protocol/tutorials/arbitrate/solver/driver
- Auction mechanism: https://docs.cow.fi/cow-protocol/reference/core/auctions
- Competition rules / deadlines: https://docs.cow.fi/cow-protocol/reference/core/auctions/competition-rules
- Auction schemas: https://docs.cow.fi/cow-protocol/reference/core/auctions/schema
- Intents / order fields: https://docs.cow.fi/cow-protocol/reference/core/intents
- Solver API: https://docs.cow.fi/cow-protocol/reference/apis/solver

## 3. LI.FI Intents / Solver Marketplace

### How it works

LI.FI Intents is a same-chain and cross-chain solver marketplace. Users lock funds through an input settler, either per-intent escrow or The Compact resource lock. Solvers publish standing quotes / inventory routes to the LI.FI order server. The order server matches user intents to available solver quotes. The winning solver delivers output assets on the destination chain, validation/oracle flow confirms delivery, and settlement releases locked input funds to the solver.

### Open question answers

- **Onboarding:** docs state LI.FI Intents is permissionless and supports permissionless solving. Solver accounts/API flows and the hosted order server should still be verified with LI.FI for production access.
- **Inventory exposure:** yes. Docs describe solvers using their own capital to deliver destination assets before settlement and publishing available inventory/quotes.
- **Latency:** order server broadcasts orders in real time; WebSocket subscription is recommended. Quote examples include 30-second intents. Escrow unlock times are often less than 2 minutes.
- **Order format / standards:** OIF-style settlement using `StandardOrder`, `MandateOutput`, input/output settlers, and EIP-7930 interoperable addresses. No ERC-7683 support found in primary docs.
- **Docs:** strong public solver, API, architecture, and settlement docs; production order-server details may require LI.FI coordination.
- **Best Lit integration:** solver fill signer, quote inventory publishing, and settlement finalization signer. Lit can validate `StandardOrder` / `MandateOutput`, quote identity, route, token, amount, recipient, deadline, oracle/settler allowlists, and inventory limits.
- **Likely contacts:** LI.FI partnership form, public team emails (`andrei@li.finance`, `julian@li.finance`), GitHub/X.

### Lit-specific thesis

LI.FI is a strong P1 because its docs frame solvers as inventory-backed entities and expose clear signer choke points: quote inventory publishing, destination fill, validation, and settlement. Lit can be a programmable signer for permissionless solvers that want fast execution without exposing inventory keys directly to bots.

### Evidence

- LI.FI Intents overview: https://docs.li.fi/lifi-intents/introduction
- Solving LI.FI Intents: https://docs.li.fi/lifi-intents/for-solvers/intro
- Orderflow / WebSocket: https://docs.li.fi/lifi-intents/for-solvers/orderflow
- Quoting / inventory routes: https://docs.li.fi/lifi-intents/for-solvers/quoting
- Auctions / order types: https://docs.li.fi/lifi-intents/for-solvers/auctions
- Filling orders: https://docs.li.fi/lifi-intents/for-solvers/filling-orders
- Settlement / `StandardOrder`: https://docs.li.fi/lifi-intents/architecture/settlement
- Partnership opportunities: https://docs.li.fi/introduction/integrating-lifi/partnership-opportunities
- LI.FI docs index: https://docs.li.fi/llms.txt

## 4. Socket / Bungee

### How it works

Bungee is a cross-chain routing API and app surface. Integrators fetch quotes, submit requests through API or Inbox contracts, and monitor request status. Bungee Auto handles execution after request submission. Deposit-address flows let users transfer funds to a generated address, after which Bungee routes funds cross-chain and handles refunds on failure.

Socket Protocol is a chain-abstraction protocol using AppGateways, Watchers, Transmitters, and Switchboards. AppGateways can orchestrate pre-execution logic such as security checks and auctions. Watchers execute off-chain logic and generate proofs; Transmitters submit proofs/user requests on-chain; Switchboards validate proofs.

### Open question answers

- **Onboarding:** Bungee API production use requires requesting API access. No public Bungee solver onboarding runbook found. Socket protocol docs say anyone can become a Watcher by running a node and anyone can write/register a Switchboard; Transmitter/solver participation appears application-dependent.
- **Inventory exposure:** Bungee solver-side inventory exposure is not clearly documented. Deposit-address flows create operational custody/refund exposure. Socket Transmitters may be relayers, solvers, fillers, provers, or paymasters, implying potential signer/capital exposure depending on the app.
- **Latency:** Bungee deposit addresses expire after **10 minutes** and are single-use. Dedicated backend default rate limit is **20 RPS**. Socket lets apps choose security/cost/latency through Switchboards.
- **Order format / standards:** Socket explicitly documents EIP/ERC-7683-style intent-based multi-chain execution, including `GaslessCrossChainOrder`. Bungee API uses its own quote/request/status model.
- **Docs:** Bungee has strong public integrator docs and API access forms; no public solver docs found. Socket has public architecture and ERC-7683 docs.
- **Best Lit integration:** for Bungee, policy-gated execution/refund/deposit operations if partner access exposes signer paths. For Socket, Lit can be a Transmitter/filler/paymaster signer or part of AppGateway/Switchboard validation before execution.
- **Likely contacts:** Bungee API access form, Socket Discord, Socket GitHub, Socket X.

### Lit-specific thesis

Socket is the better standards wedge because it explicitly discusses ERC-7683 intent execution and modular validation. Bungee is the better distribution/integrator wedge, but solver custody details are private/unknown. Outreach should ask where Bungee Auto execution is signed, whether third-party fillers/solvers participate, and whether Lit can enforce deposit/refund/fill policy at that signer boundary.

### Evidence

- Bungee integration intro: https://docs.bungee.exchange/integrate/integration-introduction
- Bungee API access: https://docs.bungee.exchange/integrate/get-api-access
- Bungee quote API: https://docs.bungee.exchange/api-reference/core-api/get-bungee-quote
- Bungee deposit flow: https://docs.bungee.exchange/integrate/integration-guides/deposit
- Bungee deposit addresses: https://docs.bungee.exchange/overview/deposit-addresses
- Bungee status codes: https://docs.bungee.exchange/integrate/integration-guides/check-status
- Socket introduction: https://docs.socket.tech/introduction
- Socket architecture: https://docs.socket.tech/architecture
- Socket ERC-7683 / intent execution: https://docs.socket.tech/eip7683
- Socket Watchers: https://docs.socket.tech/watchers
- Socket Transmitters: https://docs.socket.tech/transmitters
- Socket Switchboards: https://docs.socket.tech/switchboards

## 5. Synapse Intent Network

### How it works

Synapse Intent Network is an RFQ-based intent bridging system. Users request quotes for a bridge intent. Quoters / market makers / solvers post passive quotes or respond to active RFQs. The chosen quote builds an origin-chain bridge transaction. User funds are escrowed in FastBridge on the origin chain. A relayer fulfills on the destination chain using its own liquidity, then proves the relay back on the origin chain, waits through an optimistic dispute period, and claims escrowed funds.

FastBridgeV2 also supports temporary exclusivity: a selected relayer can receive first-fill rights for a configured number of seconds before the relay becomes open to others.

### Open question answers

- **Onboarding:** docs label relayers as permissionless, but the relaying page says current relaying requires explicit authorization and interested relayers should contact Synapse. Quoters and Provers are listed as permissioned roles.
- **Inventory exposure:** yes. Relayers provide destination-chain liquidity, approvals, and gas, then later claim origin-chain escrow reimbursement.
- **Latency:** active quoting uses WebSocket quote requests. Quoters should update quotes rapidly as prices/balances change. Exclusivity can be configured; docs say **30 seconds** is more than enough for most relays in simple implementations. Claiming is delayed by proving plus optimistic dispute period.
- **Order format / standards:** Synapse uses RFQ/FastBridge structures including encoded `BridgeTransactionV2`, `BridgeRequested` events, quote IDs, `quoteRelayer`, and `quoteExclusivitySeconds`. No ERC-7683 support found in primary docs.
- **Docs:** strong public RFQ, Quoter API, relaying, exclusivity, claiming, and contract docs. Actual relayer/quoter/prover participation is permissioned or authorized in practice.
- **Best Lit integration:** relayer `relay`, `prove`, and `claim` signers. Lit can validate `BridgeRequested` data, quote ID, exclusivity assignment, token/amount/recipient, chain IDs, inventory balances, and dispute/claim timing before signing.
- **Likely contacts:** Synapse support/docs channels, Telegram, Discord, GitHub, X.

### Lit-specific thesis

Synapse is high-fit because its docs separate quote, relay, prove, and claim roles, and FastBridgeV2 allows different addresses for those operations. Lit can provide per-role keys with different policies: low-latency relay authorization, stricter proving checks, and delayed claim batching/multicall controls.

### Evidence

- Synapse Intent Network launch: https://docs.synapseprotocol.com/blog/synapse-intent-network-launch
- RFQ overview: https://docs.synapseprotocol.com/docs/RFQ/
- Quoting: https://docs.synapseprotocol.com/docs/RFQ/Quoting/
- Quoter API: https://docs.synapseprotocol.com/docs/RFQ/Quoting/Quoter%20API/
- Bridging: https://docs.synapseprotocol.com/docs/RFQ/Bridging/
- Relaying: https://docs.synapseprotocol.com/docs/RFQ/Relaying/
- Exclusivity: https://docs.synapseprotocol.com/docs/RFQ/Exclusivity/
- Claiming: https://docs.synapseprotocol.com/docs/RFQ/Claiming/
- RFQ contracts: https://docs.synapseprotocol.com/docs/Contracts/RFQ

## 6. Router Protocol OGA

### How it works

Router OGA is a non-custodial, chain-agnostic cross-chain routing and execution graph exposed through the Xplore REST API. The graph can route across bridges, DEXs, solvers, and messaging protocols; split large orders across venues; return quotes/routes; produce wallet-ready transaction payloads; and track transaction status. Docs describe support for multi-bridge aggregation, DEX aggregation, dynamic routing, and a permissionless node registry/reputation layer.

### Open question answers

- **Onboarding:** docs describe a permissionless node registry with EIP-712 authentication and on-chain reputation. Production API keys may be required for rate limits/tracking, and docs suggest contacting the Xplore team for production deployments.
- **Inventory exposure:** mixed. Router is described as non-custodial and users keep control of funds. However, node interfaces include reserve tokens a node keeps or requires for operations, and the graph can include solver nodes.
- **Latency:** real-time quotes; advanced routes can be sorted by `TIME`; route responses include `executionDuration`. No hard solver response SLA found.
- **Order format / standards:** Xplore API route/quote/step transaction formats. No ERC-7683 support found in public OGA docs.
- **Docs:** strong public API/integration docs; production node rollout details are not fully public.
- **Best Lit integration:** policy signer for OGA node/solver inventory: validate selected route, destination chain/token/recipient/slippage/tool allowlist, reserve availability, and route ID before signing node execution or settlement transactions.
- **Likely contacts:** Router Discord, Telegram, GitHub, X, website contact/forms.

### Lit-specific thesis

Router is a strong infrastructure target if it wants external solver/node operators. Lit can be positioned as a reserve-protecting signer for nodes in the OGA graph and as a policy layer for route execution.

### Evidence

- OGA overview: https://docs.routerprotocol.com/docs/about-oga/overview/
- OGA architecture: https://docs.routerprotocol.com/docs/about-oga/architecture/
- OGA security: https://docs.routerprotocol.com/docs/about-oga/security/
- Integrate OGA: https://docs.routerprotocol.com/docs/integrate-oga/overview/
- Required interfaces: https://docs.routerprotocol.com/docs/integrate-oga/required-interfaces/
- Advanced routes API: https://docs.routerprotocol.com/docs/api-reference/advanced-routes/
- Authentication: https://docs.routerprotocol.com/docs/api-reference/authentication/
- Router site: https://routerprotocol.com/
- Router GitHub: https://github.com/router-protocol

## 7. Hashflow

### How it works

Hashflow is an RFQ liquidity network. Takers/aggregators query available market makers and price levels, then request signed quotes executed on-chain against Hashflow pools. Market makers maintain pools, publish indicative price levels over WebSocket, receive RFQs, return executable quotes, and sign quote payloads. Cross-chain quotes use Hashflow chain IDs and cross-chain messengers such as Wormhole or LayerZero.

### Open question answers

- **Onboarding:** permissioned/partnered. Market makers must request allowlisting before creating a pool; takers/aggregators must contact the Hashflow team for credentials.
- **Inventory exposure:** yes for market makers. Makers quote from their own liquidity and sign quote payloads.
- **Latency:** maker WebSocket pings every 30 seconds; makers publish price levels every second; quote expiry is enforced on-chain. No hard RFQ response SLA found.
- **Order format / standards:** Hashflow-specific RFQ/quote payloads. EVM quote signatures use EIP-191. No ERC-7683 support found.
- **Docs:** detailed public maker/taker docs, but maker pool creation and taker API access are allowlisted.
- **Best Lit integration:** market-maker quote-signing key protection. Lit verifies RFQ fields, quote expiry, pool, chain IDs, tokens, amounts, price/risk limits, and messenger allowlists before producing quote signatures.
- **Likely contacts:** Hashflow Discord, X, governance forum, website.

### Lit-specific thesis

Hashflow's maker signing flow maps directly to Lit policy signing. Lit can act as the market-maker key guard, especially for cross-chain quotes where messenger and chain IDs add more things to validate before signing.

### Evidence

- Market-making API: https://docs.hashflow.com/hashflow/market-making/getting-started-api-v3
- Market-making guides: https://docs.hashflow.com/hashflow/market-making/how-to-guides
- Taker API: https://docs.hashflow.com/hashflow/taker/getting-started-api-v3
- Official links: https://docs.hashflow.com/hashflow/company/official-links
- Hashflow site: https://hashflow.com/
- Hashflow X: https://twitter.com/hashflow
- Hashflow Discord: https://hashflow.com/discord

## 8. Bebop

### How it works

Bebop exposes RFQ and aggregation APIs. The RFQ API connects takers to private market makers quoting directly from their own on-chain inventory with firm, guaranteed-fill quotes. The Aggregation API routes trades through competing solvers for broader token coverage. Gasless mode has users sign EIP-712 orders and Bebop submits on-chain; self-execution returns transaction data for the user/integrator to broadcast. BopAMM is in closed beta for solvers, aggregators, liquidators, and market makers.

### Open question answers

- **Onboarding:** API access and production support are gated. BopAMM is closed beta with applications. Speed-optimized quotes require enablement by Bebop.
- **Inventory exposure:** yes for private market makers and likely solvers. RFQ market makers quote from their own on-chain inventory; aggregation solvers compete to find execution paths.
- **Latency:** short-expiry quotes: Ethereum 5 seconds; Arbitrum/Base/BSC 3 seconds. Standard expiries are 60-90 seconds depending chain. Speed-optimized mode targets quotes under 100 ms and requires enablement.
- **Order format / standards:** Bebop-specific RFQ and JAM/Aggregation API formats. Signing is EIP-712. No ERC-7683 support found.
- **Docs:** strong public API docs; production, BopAMM, solver application, market-maker application, and low-latency mode are partner/application-gated.
- **Best Lit integration:** PMM/solver signing and inventory guardrails. Lit can verify quote/order fields, expiry, chain, settlement contract, approval target, taker/receiver, price/risk limits, and API source before signing.
- **Likely contacts:** Bebop application Typeforms, Discord, GitHub, X, LinkedIn.

### Lit-specific thesis

Bebop is an excellent RFQ signing target because quote lifetimes are short and market makers hold inventory. Lit can protect maker keys while enforcing strict quote and inventory policy.

### Evidence

- RFQ API introduction: https://docs.bebop.xyz/rfq-api/introduction
- RFQ quote API: https://docs.bebop.xyz/rfq-api/api-reference/quote
- RFQ order API: https://docs.bebop.xyz/rfq-api/api-reference/order
- Short-expiry quotes: https://docs.bebop.xyz/rfq-api/guides/short-expiry
- Speed-optimized quotes: https://docs.bebop.xyz/rfq-api/guides/speed-optimized-quotes
- Aggregation API: https://docs.bebop.xyz/aggregation-api/introduction
- Solver use case: https://docs.bebop.xyz/use-cases/solvers
- Market-maker use case: https://docs.bebop.xyz/use-cases/market-makers
- Cross-chain aggregators: https://docs.bebop.xyz/use-cases/cross-chain-aggregators
- Bebop GitHub: https://github.com/bebop-dex

## 9. Enso

### How it works

Enso provides APIs that generate executable calldata for DeFi workflows plus a Quoter that simulates and validates arbitrary EVM transactions before signing. Cross-chain routing composes bridge actions and post-bridge callbacks for multi-chain DeFi operations using supported bridges such as CCIP, CCTP, Relay, and Stargate. Quoter's security model is simulate-then-validate: simulate a transaction on forked state, cache metadata for 5 minutes, then validate calldata/target/from/value/chain before signing.

### Open question answers

- **Onboarding:** API-key based through Enso Dashboard. No public external solver onboarding/runbook found.
- **Inventory exposure:** not direct in public docs. Enso returns executable calldata / simulation validation; users or integrators sign/broadcast. Inventory exposure exists for market makers/automation operators using Enso to rebalance or execute strategies.
- **Latency:** Quoter simulation metadata cached for 5 minutes; default API limit is 10 RPS. No solver auction timing or quote-expiry SLA found.
- **Order format / standards:** Enso-specific API request/response formats for route/bundle/quote/simulate/validate and transaction objects. No ERC-7683 support found.
- **Docs:** strong public developer docs and dashboard API-key flow. No public solver partner program found.
- **Best Lit integration:** pair Lit with Enso Quoter validation: bot asks Enso to simulate route/workflow, Lit checks validation results and private risk limits, then signs only the exact validated transaction.
- **Likely contacts:** Enso Discord, Telegram, GitHub, X, dashboard.

### Lit-specific thesis

Enso is more of a policy-validation partner than a solver custody target. A strong architecture is Enso validates the route; Lit signs only if the route, calldata, chain, amount, and private policy match exactly.

### Evidence

- Enso docs: https://docs.enso.build/home
- Authentication: https://docs.enso.build/pages/build/get-started/authentication
- Cross-chain routing: https://docs.enso.build/pages/build/get-started/crosschain-routing
- Quoter overview: https://docs.enso.build/pages/quoter/overview
- How Quoter works: https://docs.enso.build/pages/quoter/how-it-works
- Quote multiple transactions: https://docs.enso.build/api-reference/quote/quote-multiple-transactions
- Market-maker use case: https://docs.enso.build/pages/use-cases/market-makers
- Enso GitHub: https://github.com/EnsoBuild

## 10. Everclear

### How it works

Everclear is a clearing/netting layer for intents across chains. Users create intents on an origin Spoke/FeeAdapter. Solvers fill intents on destination Spokes if they have sufficient stored balance. Off-chain agents dispatch intent/fill/settlement queues to the clearing chain. Settlements are later dispatched to supported spokes based on liquidity. Everclear emphasizes reducing solver rebalancing costs by netting obligations across chains.

### Open question answers

- **Onboarding:** public contracts/API docs exist and Everclear says it makes no assumptions about solver discovery/matching/execution. No complete public production solver runbook found. Fast-path examples reference “the Everclear solver,” implying current fast-path liquidity may be operated or curated.
- **Inventory exposure:** yes for solvers/fillers. `fillIntent` debits the caller's stored balance on the destination Spoke; fast-path priority settlement/cross-chain swaps use solver inventory.
- **Latency:** priority settlement and cross-chain swap guides advertise 1-2 minute swaps / under roughly 2 minutes for Fast Path. Normal queue dispatch depends on off-chain agents and configured queue size/age thresholds.
- **Order format / standards:** Everclear-specific `Intent`, `newIntent`, and `fillIntent` contract/API format. No ERC-7683 support found in cited docs.
- **Docs:** good public concepts/API/contracts docs; production solver onboarding details are not fully public.
- **Best Lit integration:** solver `fillIntent`, settlement, and rebalance signing. Lit verifies origin intent, destination, recipient, input/output assets, amount, max fee, TTL, destination Spoke, stored balance, and fast-path liquidity policy before signing fills or liquidity moves.
- **Likely contacts:** Everclear Discord, Telegram, X, docs/DAO docs.

### Lit-specific thesis

Everclear is a strong infrastructure partner because it reduces rebalancing but still relies on solver balances. Lit can protect the signing boundary for `fillIntent`, settlement operations, and liquidity movements while Everclear optimizes clearing.

### Evidence

- Overview: https://docs.everclear.org/concepts/overview
- Getting started: https://docs.everclear.org/developers/getting-started
- Intents: https://docs.everclear.org/concepts/how-it-works/intents
- API: https://docs.everclear.org/developers/api
- Priority settlement: https://docs.everclear.org/developers/guides/priority-settlement
- Cross-chain swaps: https://docs.everclear.org/developers/guides/cross-chain-swaps
- Everclear site: https://www.everclear.org/
- Everclear Discord: https://discord.gg/everclear

## Cross-target notes

### Onboarding openness

- **Most self-serve/open in docs:** UniswapX permissionless fillers, LI.FI permissionless solving, Socket Watchers/Switchboards, Router OGA node registry.
- **Open competition but structured production onboarding:** CoW Protocol solvers.
- **Partner/API/authorization likely required:** UniswapX mainnet quoters, Bungee production API / unknown solver side, Synapse relayers/quoters/provers, Hashflow makers/takers, Bebop PMMs/solvers, Enso production API, Everclear fast-path solver operations.

### Hot inventory exposure

- **Clearly yes:** UniswapX fillers, LI.FI solvers, Synapse relayers, Hashflow market makers, Bebop PMMs/solvers, Everclear solvers/fillers.
- **Yes, but different shape:** CoW solvers use execution keys, liquidity, buffers, and settlement calldata rather than classic cross-chain destination pre-funding.
- **Mixed/depends on node:** Router OGA nodes/solvers may have reserves; Router itself is non-custodial.
- **Unknown / needs confirmation:** Bungee Auto solver inventory model, Socket Transmitter exposure by app, Enso as route infrastructure rather than solver.

### Latency posture

- **Sub-second hot path:** UniswapX RFQ quoters at 500 ms; Bebop speed-optimized quote target under 100 ms.
- **Seconds hot path:** Bebop short-expiry quotes at 3-5 seconds; Synapse RFQ/exclusivity; LI.FI quote examples around 30 seconds.
- **Block-deadline hot path:** CoW settlement deadline is chain-specific, e.g. mainnet 3 blocks.
- **Minutes / operational:** Bungee deposit address expiry at 10 minutes; LI.FI escrow unlock under 2 minutes; Everclear fast path around 1-2 minutes; Synapse claim delayed by optimistic dispute period.

### ERC-7683

- **Explicitly documented:** Socket.
- **Not found in primary docs:** UniswapX, CoW Protocol, LI.FI Intents, Bungee API, Synapse Intent Network, Router OGA, Hashflow, Bebop, Enso, Everclear.
- **Adjacent/open-intents formats:** LI.FI uses OIF-style `StandardOrder` / `MandateOutput`; UniswapX uses Permit2 + reactors; Synapse uses FastBridge/RFQ structs.

### Best Lit integration points

- **UniswapX:** filler Executor / direct-fill signer.
- **CoW:** solver driver settlement signer and buffer/hook policy.
- **LI.FI:** solver quote/fill/finalise signer.
- **Socket/Bungee:** Transmitter/filler/paymaster signer; Bungee deposit/refund/fill signer if partner access exposes it.
- **Synapse:** relayer `relay`, `prove`, and `claim` signers with role-specific policies.
- **Router OGA:** node/solver reserve signer and route execution policy.
- **Hashflow:** maker quote signer.
- **Bebop:** PMM/solver quote and fill signer.
- **Enso:** exact-validated-route signer.
- **Everclear:** `fillIntent`, settlement, and rebalance signer.
