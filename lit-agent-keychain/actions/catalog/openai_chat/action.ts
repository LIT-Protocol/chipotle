import { defineAction, requireThat } from "../../lib.ts";
type Input = {
  model: string;
  messages: { role: string; content: string }[];
  maxTokens?: number;
};
export default defineAction<Input>(async ({ credential, input, fetchJson }) => {
  const result = await fetchJson("https://api.openai.com/v1/chat/completions", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${credential}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      model: input.model,
      messages: input.messages,
      max_tokens: input.maxTokens ?? 1024,
      n: 1,
      stream: false,
    }),
  });
  const choice = result?.choices?.[0];
  const content = choice?.message?.content;
  requireThat(typeof content === "string" && content.length <= 12288);
  return {
    content,
    finishReason:
      typeof choice.finish_reason === "string"
        ? choice.finish_reason
        : "unknown",
    model: typeof result.model === "string" ? result.model : input.model,
    usage: {
      promptTokens: Number.isSafeInteger(result?.usage?.prompt_tokens)
        ? result.usage.prompt_tokens
        : 0,
      completionTokens: Number.isSafeInteger(result?.usage?.completion_tokens)
        ? result.usage.completion_tokens
        : 0,
    },
  };
});
