/**
 * Shared helpers for k6 tests.
 */
import type { Response } from "k6/http";
import { check } from "k6";

/**
 * Runs k6 checks and logs each failure to console.
 * Use instead of raw check() when you want per-check failure logging.
 */
export function checkAndLog<T>(
  val: T,
  checks: Record<string, (v: T) => boolean>,
  context?: string,
): boolean {
  let allPassed = true;
  for (const [name, fn] of Object.entries(checks)) {
    const passed = check(val, { [name]: fn });
    if (!passed) {
      let errorMsg = "";
      // Try to extract error message from typical HTTP response objects (with body)
      if (val && typeof (val as any).body === "string") {
        try {
          const parsedBody = JSON.parse((val as any).body);
          errorMsg =
            parsedBody.message ??
            parsedBody.error ??
            parsedBody.detail ??
            (typeof parsedBody === "string" ? parsedBody : JSON.stringify(parsedBody));
        } catch {
          errorMsg = (val as any).body || "(no body)";
        }
      }
      console.error(`FAIL ${name}${context ? ` (${context})` : ""}${errorMsg ? ` | ${errorMsg}` : ""}`);
      allPassed = false;
    }
  }
  return allPassed;
}

/**
 * Extract X-Correlation-Id and X-Request-Id from a response for diagnostics.
 */
function responseTraceIds(response: Response | undefined): string {
  if (!response?.headers) return "";
  const correlationId = response.headers["X-Correlation-Id"] ?? "";
  const requestId = response.headers["X-Request-Id"] ?? "";
  const parts: string[] = [];
  if (correlationId) parts.push(`correlation_id=${correlationId}`);
  if (requestId) parts.push(`request_id=${requestId}`);
  return parts.length > 0 ? ` | ${parts.join(" ")}` : "";
}

/**
 * Assert HTTP response has the expected non-2xx status (permission denial).
 * Returns true if the expected denial was confirmed, false otherwise.
 */
export function assertDenied(
  name: string,
  endpoint: string,
  res: { response: Response },
  expectedStatus = 403,
): boolean {
  const { response } = res;
  const status = response?.status ?? 0;
  const matched = status === expectedStatus;
  if (!matched) {
    let msg = "";
    try {
      const body = JSON.parse(response.body as string);
      msg = body.message ?? body.error ?? JSON.stringify(body);
    } catch {
      msg = (response?.body as string) || "(no body)";
    }
    console.error(
      `FAIL ${name} | ${endpoint} | expected ${expectedStatus} got ${status} | ${msg}${responseTraceIds(response)}`,
    );
  }
  checkAndLog(response, {
    [`${name} → ${expectedStatus}`]: (r) => (r?.status ?? 0) === expectedStatus,
  }, name);
  return matched;
}

/**
 * Assert HTTP response is NOT 2xx (any error). Use when the exact error code
 * is uncertain (e.g. contract reverts, deleted keys).
 * Returns true if the response was non-2xx, false otherwise.
 */
export function assertNon2xx(
  name: string,
  endpoint: string,
  res: { response: Response },
): boolean {
  const { response } = res;
  const status = response?.status ?? 0;
  const isError = status > 0 && (status < 200 || status >= 300);
  if (!isError) {
    console.error(
      `FAIL ${name} | ${endpoint} | expected non-2xx got ${status}${status === 0 ? " (transport error — no HTTP response)" : ""}${responseTraceIds(response)}`,
    );
  }
  checkAndLog(response, {
    [`${name} non-2xx`]: (r) => {
      const s = r?.status ?? 0;
      return s > 0 && (s < 200 || s >= 300);
    },
  }, name);
  return isError;
}

export function assertOk(
  name: string,
  endpoint: string,
  res: { response: Response },
): boolean {
  const { response } = res;
  const status = response?.status ?? 0;
  const ok = status >= 200 && status < 300;
  if (!ok) {
    let msg = "";
    if (status === 0) {
      msg = "(no response / connection failed)";
    } else {
      try {
        const body = JSON.parse(response.body as string);
        msg =
          body.message ??
          body.error ??
          body.detail ??
          (typeof body === "string" ? body : JSON.stringify(body));
      } catch {
        msg = (response.body as string) || "(no body)";
      }
    }
    console.error(`FAIL ${name} | ${endpoint} | ${status} | ${msg}${responseTraceIds(response)}`);
  }
  checkAndLog(response, {
    [`${name} 2xx`]: (r) =>
      (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, name);
  return ok;
}
