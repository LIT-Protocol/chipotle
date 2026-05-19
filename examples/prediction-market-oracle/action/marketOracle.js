// Lit Action: resolve a prediction-market question by polling up to three
// AI models in parallel and only signing a YES / NO / UNCLEAR resolution
// when every available model agrees.
//
// Why three models? Single-frontier-model resolution is too easy to
// hallucinate. Three independent models with different training and
// (importantly) one of them — Perplexity — grounded in live web search
// have to all return the same single-word answer before we attest
// anything on-chain. Same pattern as the multi-rpc-consensus-oracle
// example, applied to AI sources instead of RPC providers.
//
// Required: Perplexity (web-grounded baseline — Sonar Pro indexes the web
// at query time, so it can answer questions about events that happened
// after a frontier model's training cutoff).
// Optional: OpenAI GPT and Anthropic Claude (frontier-model second
// opinions — independent training, useful for catching Perplexity
// citation drift).
//
// If only Perplexity is configured, that's the sole resolution source.
// You'll see a `consensusAcross` array in the response listing which
// models actually voted. Consumers can decide whether a 1-of-1 resolution
// is good enough for their stakes.
//
// Two cryptographic identities are at play:
//   * The action's derived signing key (Lit.Actions.getLitActionPrivateKey)
//     — what the PredictionMarket contract pins as `oracle`.
//   * The decrypt PKP — encryption boundary for the API keys.
//
// js_params:
//   questionId               bytes32 — keccak256(bytes(text))
//   questionText             the prompt the models actually see
//   resolveAt                unix seconds; action refuses to resolve early
//   marketAddress            address of the PredictionMarket
//   marketChainId            chain id where the PredictionMarket lives
//   deadline                 signature expiry — unix seconds
//   decryptPkpId             PKP that the API keys were encrypted to
//   encryptedPerplexityKey   required — ciphertext from Lit.Actions.Encrypt
//   encryptedOpenAiKey       optional — pass null / "" / undefined to skip
//   encryptedAnthropicKey    optional — same

const ANSWER_YES = 1;
const ANSWER_NO = 2;
const ANSWER_UNCLEAR = 3;

const PROMPT = (questionText) => `You are a fact-checker resolving a prediction-market question.

Answer with EXACTLY ONE word: YES, NO, or UNCLEAR.

YES = the answer is yes / the event occurred / the statement is true.
NO = the answer is no / the event did not occur / the statement is false.
UNCLEAR = you cannot determine the answer with high confidence from authoritative sources.

If the question depends on future events that have not yet occurred, answer UNCLEAR.
If sources disagree, answer UNCLEAR.

Question: ${questionText}

Respond with the single word only. No explanation. No punctuation.`;

async function main({
  questionId,
  questionText,
  resolveAt,
  marketAddress,
  marketChainId,
  deadline,
  decryptPkpId,
  encryptedPerplexityKey,
  encryptedOpenAiKey,
  encryptedAnthropicKey,
}) {
  // Refuse to resolve before the question's resolveAt — the action enforces
  // this in addition to the contract's enforcement.
  const now = Math.floor(Date.now() / 1000);
  if (now < resolveAt) {
    return { authorized: false, reason: "question is not yet resolvable" };
  }

  // Sanity: hash the text and verify it matches the questionId. Protects
  // against a caller swapping the prompt while keeping the on-chain id.
  const computedId = ethers.utils.keccak256(
    ethers.utils.toUtf8Bytes(questionText)
  );
  if (computedId.toLowerCase() !== questionId.toLowerCase()) {
    return {
      authorized: false,
      reason: "questionText does not match questionId",
    };
  }

  if (!encryptedPerplexityKey) {
    return {
      authorized: false,
      reason: "encryptedPerplexityKey is required (web-grounded baseline)",
    };
  }

  // Decrypt whichever keys are configured, in parallel.
  const keys = await Promise.all(
    [
      ["perplexity", encryptedPerplexityKey],
      ["openai", encryptedOpenAiKey || null],
      ["anthropic", encryptedAnthropicKey || null],
    ].map(async ([name, ct]) => {
      if (!ct) return { name, key: null };
      const key = await Lit.Actions.Decrypt({
        pkpId: decryptPkpId,
        ciphertext: ct,
      });
      return { name, key };
    })
  );

  const prompt = PROMPT(questionText);

  // Fire all configured models in parallel.
  const votes = await Promise.all(
    keys.map(async ({ name, key }) => {
      if (!key) return null;
      try {
        const raw = await callModel(name, key, prompt);
        return { name, raw, vote: parseVote(raw) };
      } catch (err) {
        return { name, error: err.message };
      }
    })
  );

  const successful = votes.filter((v) => v && v.vote);
  const failed = votes.filter((v) => v && v.error);

  if (successful.length === 0) {
    return {
      authorized: false,
      reason: "no model returned a parseable answer",
      details: votes,
    };
  }

  const firstVote = successful[0].vote;
  if (!successful.every((v) => v.vote === firstVote)) {
    return {
      authorized: false,
      reason: "models disagree",
      votes: successful.map((v) => ({ name: v.name, vote: v.vote })),
    };
  }
  const answer = voteToAnswer(firstVote);

  // Sign (marketAddress, questionId, answer, deadline, chainId).
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["address", "bytes32", "uint8", "uint256", "uint256"],
      [marketAddress, questionId, answer, deadline, marketChainId]
    )
  );
  const wallet = new ethers.Wallet(await Lit.Actions.getLitActionPrivateKey());
  const signature = await wallet.signMessage(ethers.utils.arrayify(digest));

  return {
    authorized: true,
    signature,
    signer: wallet.address,
    answer,
    answerName: firstVote,
    consensusAcross: successful.map((v) => v.name),
    failedModels: failed.map((v) => ({ name: v.name, error: v.error })),
    questionId,
    deadline,
  };
}

function parseVote(text) {
  if (typeof text !== "string") return null;
  // Look for the first standalone YES / NO / UNCLEAR token.
  const m = text.toUpperCase().match(/\b(YES|NO|UNCLEAR)\b/);
  return m ? m[1] : null;
}

function voteToAnswer(vote) {
  if (vote === "YES") return ANSWER_YES;
  if (vote === "NO") return ANSWER_NO;
  return ANSWER_UNCLEAR;
}

async function callModel(name, apiKey, prompt) {
  if (name === "perplexity") return callOpenAiCompatible(
    "https://api.perplexity.ai/chat/completions",
    apiKey,
    "sonar-pro",
    prompt
  );
  if (name === "openai") return callOpenAiCompatible(
    "https://api.openai.com/v1/chat/completions",
    apiKey,
    "gpt-5.5",
    prompt
  );
  if (name === "anthropic") return callAnthropic(apiKey, prompt);
  throw new Error(`unknown model: ${name}`);
}

async function callOpenAiCompatible(url, apiKey, model, prompt) {
  const res = await fetch(url, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${apiKey}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      model,
      messages: [{ role: "user", content: prompt }],
      max_tokens: 16,
      temperature: 0,
    }),
  });
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  const body = await res.json();
  return body?.choices?.[0]?.message?.content || "";
}

async function callAnthropic(apiKey, prompt) {
  const res = await fetch("https://api.anthropic.com/v1/messages", {
    method: "POST",
    headers: {
      "x-api-key": apiKey,
      "anthropic-version": "2023-06-01",
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      model: "claude-opus-4-7",
      max_tokens: 16,
      temperature: 0,
      messages: [{ role: "user", content: prompt }],
    }),
  });
  if (!res.ok) throw new Error(`anthropic returned ${res.status}`);
  const body = await res.json();
  return body?.content?.[0]?.text || "";
}
