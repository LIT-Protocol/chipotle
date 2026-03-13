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
 * Assert HTTP response is 2xx. Logs failure and runs checkAndLog for k6 metrics.
 * Returns true if ok, false otherwise.
 */
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
    console.error(`FAIL ${name} | ${endpoint} | ${status} | ${msg}`);
  }
  checkAndLog(response, {
    [`${name} 2xx`]: (r) =>
      (r?.status ?? 0) >= 200 && (r?.status ?? 0) < 300,
  }, name);
  return ok;
}
