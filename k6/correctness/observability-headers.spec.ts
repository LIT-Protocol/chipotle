/**
 * Correctness tests for the ObservabilityFairing response headers.
 *
 * Verifies the X-Request-Id and X-Correlation-Id contract defined in
 * architectureDocs/deployment/planning/observability/requirements.md
 *
 * Tests against both a simple endpoint (get_node_chain_config) and the
 * lit_action endpoint, which exercises the full request lifecycle including
 * the handoff from lit-api-server → lit-actions gRPC → response.
 *
 * Usage:
 *   k6 run k6/correctness/observability-headers.spec.ts
 *   BASE_URL=https://your-instance/core/v1 k6 run k6/correctness/observability-headers.spec.ts
 */
import http from "k6/http";
import { checkAndLog } from "../helpers.ts";
import { LitApiServerClient } from "../litApiServer.ts";
import { PRECREATED_ACCOUNTS } from "../setup.ts";
import { assertOk } from "../helpers.ts";
import { HELLO_WORLD_CODE } from "../LitActionCode/index.ts";
import { BASE_URL, COMMON_PARAMS } from "../defaults.ts";

const SIMPLE_ENDPOINT = `${BASE_URL}/get_node_chain_config`;

// UUID v4 pattern: 8-4-4-4-12 hex digits, version nibble = 4, variant bits = 8/9/a/b
const UUID_V4_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    http_req_failed: ["rate<0.1"],
    http_req_duration: ["p(99)<30000"],
    http_reqs: ["count>=1"],
    checks: ["rate==1"],
  },
};

export interface ObservabilitySetupData {
  usageApiKey: string;
}

export function setup(): ObservabilitySetupData {
  if (PRECREATED_ACCOUNTS.length === 0) {
    throw new Error(
      "No pre-created accounts found. Run accounts.seed.spec.ts first to generate k6/data/accounts.json",
    );
  }
  const account =
    PRECREATED_ACCOUNTS[Math.floor(Math.random() * PRECREATED_ACCOUNTS.length)];
  return { usageApiKey: account.usageApiKey };
}

export default function (data: ObservabilitySetupData) {
  const { usageApiKey } = data;
  const client = new LitApiServerClient({ baseUrl: BASE_URL, commonRequestParameters: COMMON_PARAMS });

  // ── 1. Simple endpoint: X-Request-Id is present and UUID v4 ────────────
  {
    const res = http.get(SIMPLE_ENDPOINT);
    checkAndLog(res, {
      "simple: status 2xx": (r) => r.status >= 200 && r.status < 300,
      "simple: X-Request-Id is present": (r) =>
        r.headers["X-Request-Id"] !== undefined &&
        r.headers["X-Request-Id"] !== "",
      "simple: X-Request-Id is UUID v4": (r) =>
        UUID_V4_RE.test(r.headers["X-Request-Id"] || ""),
    }, "simple baseline");
  }

  // ── 2. Each request gets a unique X-Request-Id ─────────────────────────
  {
    const res1 = http.get(SIMPLE_ENDPOINT);
    const res2 = http.get(SIMPLE_ENDPOINT);
    const id1 = res1.headers["X-Request-Id"];
    const id2 = res2.headers["X-Request-Id"];
    checkAndLog(res1, {
      "two requests produce different X-Request-Id": () =>
        id1 !== undefined && id2 !== undefined && id1 !== id2,
    }, "uniqueness");
  }

  // ── 3. User-provided X-Request-Id is ignored ──────────────────────────
  {
    const userRequestId = "550e8400-e29b-41d4-a716-446655440000";
    const res = http.get(SIMPLE_ENDPOINT, {
      headers: { "X-Request-Id": userRequestId },
    });
    checkAndLog(res, {
      "user X-Request-Id ignored: response differs": (r) =>
        r.headers["X-Request-Id"] !== userRequestId,
      "user X-Request-Id ignored: still UUID v4": (r) =>
        UUID_V4_RE.test(r.headers["X-Request-Id"] || ""),
    }, "ignore user X-Request-Id");
  }

  // ── 4. X-Correlation-Id echoed when user provides it ──────────────────
  {
    const userCorrelationId = "my-correlation-abc-123";
    const res = http.get(SIMPLE_ENDPOINT, {
      headers: { "X-Correlation-Id": userCorrelationId },
    });
    checkAndLog(res, {
      "simple: X-Correlation-Id echoed back": (r) =>
        r.headers["X-Correlation-Id"] === userCorrelationId,
      "simple: X-Request-Id still present alongside correlation": (r) =>
        UUID_V4_RE.test(r.headers["X-Request-Id"] || ""),
    }, "echo X-Correlation-Id");
  }

  // ── 5. X-Correlation-Id absent when user does not send it ─────────────
  {
    const res = http.get(SIMPLE_ENDPOINT);
    checkAndLog(res, {
      "X-Correlation-Id absent when not sent": (r) =>
        r.headers["X-Correlation-Id"] === undefined ||
        r.headers["X-Correlation-Id"] === "",
    }, "no X-Correlation-Id");
  }

  // ── 6. Empty X-Correlation-Id treated as absent ───────────────────────
  {
    const res = http.get(SIMPLE_ENDPOINT, {
      headers: { "X-Correlation-Id": "" },
    });
    checkAndLog(res, {
      "empty X-Correlation-Id not echoed": (r) =>
        r.headers["X-Correlation-Id"] === undefined ||
        r.headers["X-Correlation-Id"] === "",
    }, "empty X-Correlation-Id");
  }

  // ── 7. lit_action: headers survive api-server → lit-actions → response ─
  {
    const userCorrelationId = "e2e-lit-action-corr-456";
    const laRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      { "X-Api-Key": usageApiKey } as any,
      { headers: { "X-Correlation-Id": userCorrelationId } },
    );
    if (!assertOk("lit_action+corr", "POST /lit_action", laRes)) return;
    checkAndLog(laRes.response, {
      "lit_action: X-Request-Id present": (r) =>
        r.headers["X-Request-Id"] !== undefined &&
        r.headers["X-Request-Id"] !== "",
      "lit_action: X-Request-Id is UUID v4": (r) =>
        UUID_V4_RE.test(r.headers["X-Request-Id"] || ""),
      "lit_action: X-Correlation-Id echoed": (r) =>
        r.headers["X-Correlation-Id"] === userCorrelationId,
      "lit_action: response body has no error": (r) => {
        try {
          return JSON.parse(r.body as string).has_error === false;
        } catch {
          return false;
        }
      },
    }, "lit_action with headers");
  }

  // ── 8. lit_action: no correlation header when not sent ─────────────────
  {
    // Use a client without COMMON_PARAMS so X-Correlation-Id is truly absent.
    const bareClient = new LitApiServerClient({ baseUrl: BASE_URL });
    const laRes = bareClient.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      { "X-Api-Key": usageApiKey } as any,
    );
    if (!assertOk("lit_action no-corr", "POST /lit_action", laRes)) return;
    checkAndLog(laRes.response, {
      "lit_action no-corr: X-Request-Id present": (r) =>
        UUID_V4_RE.test(r.headers["X-Request-Id"] || ""),
      "lit_action no-corr: X-Correlation-Id absent": (r) =>
        r.headers["X-Correlation-Id"] === undefined ||
        r.headers["X-Correlation-Id"] === "",
    }, "lit_action without correlation");
  }

  // ── 9. lit_action: user X-Request-Id ignored through full roundtrip ───
  {
    const userRequestId = "550e8400-e29b-41d4-a716-446655440000";
    const laRes = client.litAction(
      { code: HELLO_WORLD_CODE, js_params: null },
      { "X-Api-Key": usageApiKey } as any,
      { headers: { "X-Request-Id": userRequestId } },
    );
    if (!assertOk("lit_action ignore-rid", "POST /lit_action", laRes)) return;
    checkAndLog(laRes.response, {
      "lit_action: user X-Request-Id ignored": (r) =>
        r.headers["X-Request-Id"] !== userRequestId,
      "lit_action: server X-Request-Id is UUID v4": (r) =>
        UUID_V4_RE.test(r.headers["X-Request-Id"] || ""),
    }, "lit_action ignore user X-Request-Id");
  }
}
