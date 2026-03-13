/**
 * Shared defaults for k6 tests.
 * BASE_URL can be overridden via the BASE_URL environment variable.
 */
export const BASE_URL =
  __ENV.BASE_URL || "https://api.dev.litprotocol.com/core/v1";
