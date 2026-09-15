import { defineAction, pathSegment, requireThat } from "../../lib.ts";
type Input = { owner: string; repo: string; path: string; ref?: string };
const MAX_CONTENT = 14336;
export default defineAction<Input>(async ({ credential, input, fetchJson }) => {
  const path = input.path.split("/").map((segment) => pathSegment(segment));
  const url = new URL(
    `https://api.github.com/repos/${pathSegment(input.owner)}/${pathSegment(input.repo)}/contents/${path.join("/")}`,
  );
  if (input.ref !== undefined) url.searchParams.set("ref", input.ref);
  const result = await fetchJson(url.href, {
    headers: {
      Authorization: `Bearer ${credential}`,
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      "User-Agent": "lit-agent-keychain",
    },
  });
  requireThat(
    result?.type === "file" &&
      typeof result.sha === "string" &&
      Number.isSafeInteger(result.size) &&
      typeof result.path === "string" &&
      result.encoding === "base64" &&
      typeof result.content === "string",
  );
  const bytes = Uint8Array.from(atob(result.content.replace(/\s+/g, "")), (c) =>
    c.charCodeAt(0),
  );
  // Text only: a decode failure (binary file) denies rather than returning garbage.
  const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  const truncated = text.length > MAX_CONTENT;
  return {
    path: result.path,
    sha: result.sha,
    size: result.size,
    content: truncated ? text.slice(0, MAX_CONTENT) : text,
    truncated,
  };
});
