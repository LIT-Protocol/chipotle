/**
 * Correctness test for the billing configuration endpoint.
 *
 * Verifies that GET /billing/stripe_config returns a Stripe publishable key,
 * confirming that Stripe secrets were injected and billing is enabled on the
 * deployed server.
 *
 * Usage:
 *   k6 run k6/correctness/billing.spec.ts
 *   BASE_URL=https://your-instance/core/v1 k6 run k6/correctness/billing.spec.ts
 */
import http from "k6/http";
import { checkAndLog, warnOnHttpFailures } from "../helpers.ts";
import { BASE_URL } from "../defaults.ts";

const STRIPE_CONFIG_ENDPOINT = `${BASE_URL}/billing/stripe_config`;

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_req_duration: ["p(99)<30000"],
    http_reqs: ["count>=1"],
    checks: ["rate==1"],
  },
};

export default function () {
  // ── 1. GET /billing/stripe_config returns 200 with publishable key ────
  {
    const res = http.get(STRIPE_CONFIG_ENDPOINT);
    checkAndLog(
      res,
      {
        "stripe_config: status 200": (r) => r.status === 200,
        "stripe_config: has publishable_key": (r) => {
          try {
            const body = JSON.parse(r.body as string);
            return (
              typeof body.publishable_key === "string" &&
              body.publishable_key.length > 0
            );
          } catch {
            return false;
          }
        },
        "stripe_config: publishable_key starts with pk_": (r) => {
          try {
            const body = JSON.parse(r.body as string);
            return body.publishable_key.startsWith("pk_");
          } catch {
            return false;
          }
        },
      },
      "billing stripe_config",
    );
  }
}

export const handleSummary = warnOnHttpFailures;
