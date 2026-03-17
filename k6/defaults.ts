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
 */
export const K6_RUN_ID = `k6-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

/**
 * Common request parameters applied to every LitApiServerClient instance.
 * Injects the run-specific X-Correlation-Id header.
 */
export const COMMON_PARAMS: Params = {
  headers: {
    "X-Correlation-Id": K6_RUN_ID,
  },
};
