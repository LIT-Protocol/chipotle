// The only module a catalog action may import. It exposes exactly what an action
// needs to use a credential: the decoded value, the validated agent input, and an
// HTTP client that the harness has already bound to the action's declared hosts,
// method set, request count, timeout and response size. Everything else (the Lit
// runtime, the raw request, global fetch, timers) is out of reach by construction,
// and the build rejects actions that reference them.
export type { Shape } from "./catalog/shape.ts";
export { requireThat } from "../protocol/crypto.ts";

export type ActionMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
export type ActionRequestInit = {
  method?: ActionMethod;
  headers?: Record<string, string>;
  body?: string;
};
export type ActionContext<Input = unknown> = {
  /** The decrypted credential, already matched against `credentialPattern`. */
  credential: string;
  /** The agent's request input, validated against the manifest's `input` shape. */
  input: Input;
  /** HTTPS JSON request to an allowed host. Non-2xx, redirects and oversize bodies throw. */
  fetchJson: (url: string, init?: ActionRequestInit) => Promise<any>;
  /** As `fetchJson`, returning the raw UTF-8 body. */
  fetchText: (url: string, init?: ActionRequestInit) => Promise<string>;
};
/**
 * An action projects the upstream response onto the manifest's `output` shape and
 * returns it. Throwing anything denies the request; the agent only ever sees
 * `access_denied`, never an upstream error, header or reflected string.
 */
export type ActionUse<Input = unknown> = (
  context: ActionContext<Input>,
) => Promise<unknown>;
export const defineAction = <Input = unknown>(use: ActionUse<Input>) => use;
/** Percent-encodes one path segment and rejects anything that could change the route. */
export function pathSegment(value: unknown, maxLength = 128): string {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.length > maxLength ||
    value === "." ||
    value === ".."
  )
    throw new Error("Invalid path segment");
  return encodeURIComponent(value);
}
