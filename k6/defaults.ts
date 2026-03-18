/**
 * Shared defaults for k6 tests.
 * BASE_URL can be overridden via the BASE_URL environment variable.
 */
import type { Params } from "k6/http";

export const BASE_URL =
  __ENV.BASE_URL || "https://api.dev.litprotocol.com/core/v1";

/**
 * A unique ID for this k6 run, used as the X-Correlation-Id header so that
 * every request from a single run can be correlated in server-side logs.
 *
 * Pass K6_CORRELATION_ID as an env var to ensure the same ID is used across
 * all k6 phases (init, setup, VU, teardown). The justfile recipes do this
 * automatically. Without it, each phase generates its own ID.
 */
export const K6_RUN_ID =
  __ENV.K6_CORRELATION_ID ||
  `k6-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

/**
 * Common request parameters applied to every LitApiServerClient instance.
 * Injects the run-specific X-Correlation-Id header.
 */
export const COMMON_PARAMS: Params = {
  headers: {
    "X-Correlation-Id": K6_RUN_ID,
  },
};
