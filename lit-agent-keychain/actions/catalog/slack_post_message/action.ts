import { defineAction, requireThat } from "../../lib.ts";
type Input = { channel: string; text: string; threadTs?: string };
export default defineAction<Input>(async ({ credential, input, fetchJson }) => {
  const result = await fetchJson("https://slack.com/api/chat.postMessage", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${credential}`,
      "Content-Type": "application/json; charset=utf-8",
    },
    body: JSON.stringify({
      channel: input.channel,
      text: input.text,
      ...(input.threadTs !== undefined ? { thread_ts: input.threadTs } : {}),
    }),
  });
  // Slack reports failures with HTTP 200 and ok:false; never reflect its error string.
  requireThat(
    result?.ok === true &&
      typeof result.channel === "string" &&
      typeof result.ts === "string",
  );
  return { channel: result.channel, ts: result.ts };
});
