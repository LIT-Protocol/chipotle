/**
 * Shared pieces for the venue conformance specs (plan D5/M1/M2.5).
 *
 * Each spec executes the lit-venues IIFE bundle INSIDE a real Lit Action on
 * the target environment — this is the gate tier above unit tests and Node
 * live-conformance: real runtime, real egress, real venue.
 */
import { LitApiServerClient } from "../litApiServer.ts";
import { BASE_URL, COMMON_PARAMS } from "../defaults.ts";
import { PRECREATED_ACCOUNTS } from "../setup.ts";
import { ensureAccountCredits } from "../stripe.ts";
import { assertOk } from "../helpers.ts";

// k6's init-context file reader.
declare const open: (path: string) => string;

/** Load the inline bundle. CI builds lit-venues first (`npm run build`). */
export function loadVenuesBundle(): string {
  const candidates = [
    "../../lit-venues/dist/lit-venues.iife.js", // main file in k6/correctness/
    "../lit-venues/dist/lit-venues.iife.js", //    main file in k6/
    "lit-venues/dist/lit-venues.iife.js", //       cwd = repo root
  ];
  for (const path of candidates) {
    try {
      return open(path);
    } catch (_e) {
      // try the next layout
    }
  }
  throw new Error(
    "lit-venues bundle not found — run `npm run build` in lit-venues/ before this spec",
  );
}

export interface VenueSpecContext {
  usageKeyHeaders: { "X-Api-Key": string };
}

export function venueSpecSetup(): VenueSpecContext {
  if (PRECREATED_ACCOUNTS.length === 0) {
    throw new Error(
      "No pre-created accounts found. Run accounts.seed.spec.ts first to generate k6/data/accounts.json",
    );
  }
  const account =
    PRECREATED_ACCOUNTS[Math.floor(Math.random() * PRECREATED_ACCOUNTS.length)];
  const client = new LitApiServerClient({
    baseUrl: BASE_URL,
    commonRequestParameters: COMMON_PARAMS,
  });
  ensureAccountCredits(client, { "X-Api-Key": account.apiKey });
  return { usageKeyHeaders: { "X-Api-Key": account.usageApiKey } };
}

/** Run an action (bundle + main) and return the parsed response, or null after logging. */
export function runVenueAction(
  label: string,
  headers: { "X-Api-Key": string },
  bundle: string,
  mainCode: string,
  jsParams: unknown,
): Record<string, unknown> | null {
  const client = new LitApiServerClient({
    baseUrl: BASE_URL,
    commonRequestParameters: COMMON_PARAMS,
  });
  const res = client.litAction(
    { code: `${bundle}\n${mainCode}`, js_params: jsParams ?? null },
    headers,
  );
  if (!assertOk(label, "POST /lit_action", res)) return null;
  const body = JSON.parse(res.response.body as string) as {
    has_error: boolean;
    logs?: string;
    response: Record<string, unknown>;
  };
  if (body.has_error) {
    console.error(`${label}: action reported an error; logs: ${body.logs ?? "<none>"}`);
    return null;
  }
  return body.response;
}

export const VENUE_SPEC_OPTIONS = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_req_duration: ["p(99)<60000"],
    checks: ["rate==1"],
  },
};
