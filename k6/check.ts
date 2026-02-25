/**
 * checkAndLog — runs k6 checks and logs each failure to console.
 * Use instead of raw check() when you want per-check failure logging.
 */
import { check } from "k6";

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
