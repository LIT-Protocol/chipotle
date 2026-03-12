/**
 * Correctness tests for the ObservabilityFairing response headers.
 *
 * Verifies the X-Request-Id and X-Correlation-Id contract defined in
 * architectureDocs/deployment/planning/observability/requirements.md
 *
 * Uses the public GET /get_node_chain_config endpoint (no auth required).
 *
 * Usage:
 *   k6 run k6/correctness/observability-headers.spec.ts
 *   BASE_URL=https://your-instance/core/v1 k6 run k6/correctness/observability-headers.spec.ts
 */
import http from "k6/http";
import { checkAndLog } from "../check.ts";

const BASE_URL =
  __ENV.BASE_URL ||
  "https://e364da71b0c9af3b9068daa6321edd6ee932aa89-8000.dstack-pha-prod5.phala.network/core/v1";

const ENDPOINT = `${BASE_URL}/get_node_chain_config`;

// UUID v4 pattern: 8-4-4-4-12 hex digits, version nibble = 4, variant bits = 8/9/a/b
const UUID_V4_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    checks: ["rate==1"],
  },
};

export default function () {
  // ── 1. X-Request-Id is always present and is a UUID v4 ────────────────────
  {
    const res = http.get(ENDPOINT);
    checkAndLog(res, {
      "baseline: status 2xx": (r) => r.status >= 200 && r.status < 300,
      "X-Request-Id is present": (r) =>
        r.headers["X-Request-Id"] !== undefined &&
        r.headers["X-Request-Id"] !== "",
      "X-Request-Id is UUID v4": (r) =>
        UUID_V4_RE.test(r.headers["X-Request-Id"] || ""),
    }, "baseline request");
  }

  // ── 2. Each request gets a unique X-Request-Id ────────────────────────────
  {
    const res1 = http.get(ENDPOINT);
    const res2 = http.get(ENDPOINT);
    const id1 = res1.headers["X-Request-Id"];
    const id2 = res2.headers["X-Request-Id"];
    checkAndLog(res1, {
      "two requests produce different X-Request-Id": () =>
        id1 !== undefined && id2 !== undefined && id1 !== id2,
    }, "uniqueness");
  }

  // ── 3. User-provided X-Request-Id is ignored ──────────────────────────────
  {
    const userRequestId = "550e8400-e29b-41d4-a716-446655440000";
    const res = http.get(ENDPOINT, {
      headers: { "X-Request-Id": userRequestId },
    });
    checkAndLog(res, {
      "user X-Request-Id ignored: response differs": (r) =>
        r.headers["X-Request-Id"] !== userRequestId,
      "user X-Request-Id ignored: still UUID v4": (r) =>
        UUID_V4_RE.test(r.headers["X-Request-Id"] || ""),
    }, "ignore user X-Request-Id");
  }

  // ── 4. X-Correlation-Id echoed when user provides it ──────────────────────
  {
    const userCorrelationId = "my-correlation-abc-123";
    const res = http.get(ENDPOINT, {
      headers: { "X-Correlation-Id": userCorrelationId },
    });
    checkAndLog(res, {
      "X-Correlation-Id echoed back": (r) =>
        r.headers["X-Correlation-Id"] === userCorrelationId,
      "X-Request-Id still present alongside correlation": (r) =>
        UUID_V4_RE.test(r.headers["X-Request-Id"] || ""),
    }, "echo X-Correlation-Id");
  }

  // ── 5. X-Correlation-Id absent when user does not send it ─────────────────
  {
    const res = http.get(ENDPOINT);
    checkAndLog(res, {
      "X-Correlation-Id absent when not sent": (r) =>
        r.headers["X-Correlation-Id"] === undefined ||
        r.headers["X-Correlation-Id"] === "",
    }, "no X-Correlation-Id");
  }

  // ── 6. Empty X-Correlation-Id treated as absent ───────────────────────────
  {
    const res = http.get(ENDPOINT, {
      headers: { "X-Correlation-Id": "" },
    });
    checkAndLog(res, {
      "empty X-Correlation-Id not echoed": (r) =>
        r.headers["X-Correlation-Id"] === undefined ||
        r.headers["X-Correlation-Id"] === "",
    }, "empty X-Correlation-Id");
  }
}
