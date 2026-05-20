# Prediction Market Oracle (AI Consensus)

**Resolve yes/no prediction-market questions on-chain by polling multiple AI
models and only signing the answer when they all agree.**

This is the "use AI on-chain via Lit" example. Single-frontier-model
resolution is too easy to hallucinate; this example uses the same
multi-source pattern as [`../multi-source-price-oracle`](../multi-source-price-oracle),
but the parallel sources are AI providers instead of price feeds.
Strict agreement (rather than median) is used here because the output
is categorical YES/NO/UNCLEAR — there's nothing to take a median of.

- **Perplexity Sonar Pro — required.** Sonar indexes the web at query time,
  so it can answer questions about events that happened after a frontier
  model's training cutoff. This is the baseline.
- **OpenAI (`gpt-5.5`) — optional.** Frontier-model second opinion.
  Independent training set from Perplexity.
- **Anthropic (`claude-opus-4-7`) — optional.** Another independent
  frontier model.

The action calls every configured model in parallel with the same prompt
and only signs the resolution if every successful response is the same
YES / NO / UNCLEAR. If only Perplexity is configured, that's the sole
source; the signed result carries a single vote — fine for low-stakes
demos, weak for real money. Configuring all three gives you 3-of-3
agreement before anything reaches the chain.

## How it works

```
  market.propose("Will 2027 be a leap year?", resolveAt)
       │
       ▼
  PredictionMarket (Base, or wherever you deploy it)
       │
       │  later — anyone calls market.resolve(...)
       │
   resolver script
       │
       │ ask the Lit Action
       ▼
   Lit Action ──┐ Perplexity Sonar Pro (web-grounded) ──► "NO"
                │
                ├ OpenAI gpt-5.5 (optional)             ──► "NO"
                │
                └ Anthropic claude-opus-4-7 (optional)  ──► "NO"
                                                            │
                                  all agree → sign answer   │
                                  with                      │
                                  getLitActionPrivateKey()  │
                                                            │
                                            ◄───────────────┘
       │
       ▼
  market.resolve(id, NO, deadline, sig)
       │
       ▼   ecrecover(sig) == oracle ✓  →  question stored as NO
```

The signature uses `Lit.Actions.getLitActionPrivateKey()` — derived from
the action's IPFS CID. The deployed `PredictionMarket` pins the address
of that key as its `oracle`. Edit the action by a byte and the address
changes; old markets stop trusting the new action. This is the same
action-identity-signing story as the compliance and multi-RPC examples.

## Why Perplexity is required

Frontier models like GPT and Claude have training cutoffs. For
"Did X happen?" questions about events after their cutoff, they can
either say "I don't know" (UNCLEAR, fine) or hallucinate (bad). Perplexity
runs a web search behind every prompt, so it has live information. The
example treats Perplexity as the *factual* source and the frontier models
as *consistency checks* on top of it.

Configurations that make sense in practice:

| Setup                        | Use case                                       |
| ---------------------------- | ---------------------------------------------- |
| Perplexity only              | Local demos, very-low-stakes resolution.       |
| Perplexity + OpenAI          | Real questions, moderate stakes.               |
| Perplexity + OpenAI + Claude | High stakes — 3 independent models must agree. |

You can flip between configurations any time by adding/removing keys in
`.env` and re-running `npm run setup`; step 9 re-encrypts whatever's
present.

## Files

| Path | Purpose |
| --- | --- |
| `action/marketOracle.js` | The Lit Action: decrypts available API keys, asks each model in parallel, requires consensus, signs the resolution. |
| `contracts/PredictionMarket.sol` | Minimal `propose` + `resolve` registry. Pure resolution-attestation contract — no betting logic. |
| `scripts/setup.js` | One-shot setup: mints PKP, computes action CID, derives action wallet address, creates and wires the group, deploys the contract, encrypts every configured AI provider key. Idempotent. |
| `scripts/mintPkp.js` | Mints the decrypt PKP (called by setup). |
| `scripts/encryptApiKeys.js` | Encrypts whichever AI keys are present in `.env` to the decrypt PKP (called by setup). |
| `scripts/deploy.js` | Hardhat deploy; pins the action's derived address as `oracle` (called by setup). |
| `scripts/propose.js` | Convenience CLI for `market.propose(text, resolveAt)`. |
| `scripts/resolve.js` | End-to-end runner: read question text from chain, ask the action, submit `market.resolve(id, answer, sig)`. |
| `scripts/_env.js` | Tiny shared helper for `.env` read + upsert. |
| `.env.example` | All the env vars you'll fill in. |

## Walkthrough

### 1. Get API keys

- **Perplexity (required):** sign up at https://www.perplexity.ai/api-platform.
  Keys issue instantly with a payment method on file.
- **OpenAI (optional):** https://platform.openai.com/api-keys.
- **Anthropic (optional):** https://console.anthropic.com/account/keys.

### 2. Fill in your inputs

```bash
cp .env.example .env
npm install
```

Edit `.env` and set:
- `LIT_API_KEY` — your **account-level (master) API key** from the
  [Chipotle dashboard](https://dashboard.chipotle.litprotocol.com), *not* a
  scoped usage key. Setup calls management endpoints that revert
  `NotMasterAccount` on scoped keys.
- `PERPLEXITY_API_KEY` — required.
- `OPENAI_API_KEY`, `ANTHROPIC_API_KEY` — optional.
- `DEPLOYER_PRIVATE_KEY` — an EOA with gas on Base Sepolia (or your target chain).
- `PROPOSER_PRIVATE_KEY` and `RESOLVER_PRIVATE_KEY` — for the
  `propose.js` and `resolve.js` flows; can be the same EOA as the deployer
  for testing.

### 3. Run setup

```bash
npm run setup
```

Walks through ten steps, printing each as it goes. The headline pieces:

- Step 1 mints a **decrypt PKP** used as the encryption boundary for the
  AI provider keys (Lit's `Encrypt`/`Decrypt` are PKP-keyed).
- Step 3 creates a group with a **wildcard action allowlist**
  (`cid_hashes_permitted: ["0"]`) so the deriver and the encrypt-helper
  inline actions can execute.
- Step 5 creates a **scoped usage API key** with execute permission in
  the group, saved as `LIT_USAGE_API_KEY`. Steps 6 + 10 plus `resolve.js`
  call `/lit_action` with this key — the master `LIT_API_KEY` can't
  execute actions in your own groups (the contract's `canExecuteAction`
  only consults `usageApiKeys[...]`).

Re-running `npm run setup` does a fresh setup top-to-bottom: every step
creates new on-chain state and overwrites the corresponding key in
`.env`. The previously-minted PKP / group / usage key / contract / and
ciphertexts become orphaned.

### 4. Propose and resolve a question

```bash
# Propose a question. resolveAt defaults to "now + 5 minutes" so you
# can immediately resolve it.
npm run propose -- --text "Will the year 2027 be a leap year?"
#                                                  ↳ 2027 % 4 != 0, so the
#                                                    correct answer is NO

# Wait ~5 minutes (or pass --resolveIn 10 to propose with a 10-second window
# for quick demos).

npm run resolve -- --id 0x<the-id-printed-above>
```

Expected output for a clear answer:

```
Question: Will the year 2027 be a leap year?
resolveAt: 2026-05-19T22:00:00.000Z
Asking the AI consensus oracle...
Consensus: NO (across perplexity, openai, anthropic)
tx: 0x...
mined in block 12345678
```

If the models disagree, the action refuses to sign:

```
Action declined to sign: {
  authorized: false,
  reason: 'models disagree',
  votes: [
    { name: 'perplexity', vote: 'YES' },
    { name: 'openai', vote: 'UNCLEAR' }
  ]
}
```

## Honest limitations

The "AI consensus" pattern is much better than a single-model lookup but
it isn't trustless. Correlated failure modes worth being explicit about:

- **Shared training data.** Frontier models are trained on overlapping
  internet corpora. If a wrong answer is widespread in those corpora,
  multiple models can confidently agree on the same wrong answer.
- **Perplexity citation drift.** Perplexity's grounding only helps when
  its search returns authoritative sources. For obscure or rapidly-moving
  questions you can get plausible-looking citations that don't actually
  support the claim.
- **Prompt parsing.** The action does a regex match for the first YES /
  NO / UNCLEAR token. A model that prefaces its answer with stray text
  containing one of those words could be parsed wrong. (`temperature: 0`
  and the "respond with the single word only" prompt make this very
  unlikely, but not impossible.)
- **Provider outages.** If a provider returns a 5xx, the action treats
  that model as having no vote rather than UNCLEAR. With all three
  configured, two-out-of-three agreement still authorizes a resolution
  when one provider is down — review whether that matches your trust
  model and tighten the action if not.

For real money you'd layer this with: a dispute window (let humans veto
within N hours), a stake-and-slash flow for the proposer, or pair the
resolution with verifiable web data when the question allows.

## Production considerations

- **Cost.** Resolving a question hits up to three paid LLM APIs. With
  short `max_tokens: 16` prompts the cost is on the order of fractions
  of a cent per resolution per model.
- **Latency.** All three providers are called in parallel; total
  resolution latency is roughly `max(model_latency)` plus the Lit Action
  overhead. Typically a few seconds.
- **Upgrades.** Because the oracle address is derived from the action's
  CID, swapping models or tightening the prompt produces a new oracle
  address. Old `PredictionMarket` deployments will refuse signatures
  from the new action; the upgrade path is either redeploy or add a
  rotate-oracle setter behind a multisig.
- **Replay.** This contract resolves each question once and ignores any
  subsequent calls. Combined with a per-call `deadline`, replay attacks
  on stale signatures are not useful.
