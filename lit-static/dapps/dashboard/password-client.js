import { validateParameters } from "./password-protocol.js";
const configured = "__LIT_AUTH_BASE_URL__";
export function authBaseUrl() {
  // A local override is deliberately restricted to loopback development origins.
  const local = ["localhost", "127.0.0.1"].includes(location.hostname);
  const value = configured.startsWith("__")
    ? local
      ? "http://localhost:8787"
      : ""
    : configured;
  if (!value) return "";
  const url = new URL(value);
  if (
    url.protocol !== "https:" &&
    !(local && ["localhost", "127.0.0.1"].includes(url.hostname))
  )
    return "";
  return url.origin;
}
export const enabled = () => !!authBaseUrl();
let current = null;
let signingOut = null;
export const hasAuthSession = () => current !== null;
export function clearAuthSession() {
  current = null;
}
export async function request(path, body, method = "POST") {
  if (signingOut) await signingOut;
  return send(path, body, method);
}
async function send(path, body, method = "POST") {
  if (!enabled())
    throw new Error("Email sign-in is not available on this dashboard.");
  let response;
  try {
    response = await fetch(`${authBaseUrl()}/auth/v1/${path}`, {
      method,
      credentials: "include",
      cache: "no-store",
      signal: AbortSignal.timeout(30000),
      headers: {
        "X-Chipotle-Auth": "1",
        ...(method !== "GET" ? { "Content-Type": "application/json" } : {}),
        ...(current?.csrf ? { "X-CSRF-Token": current.csrf } : {}),
      },
      ...(method !== "GET"
        ? {
            body: JSON.stringify({
              ...(current?.parameters
                ? {
                    id: current.parameters.id,
                    version: current.parameters.version,
                  }
                : {}),
              ...body,
            }),
          }
        : {}),
    });
  } catch (e) {
    throw new Error(
      e?.name === "TimeoutError" || e?.name === "AbortError"
        ? "Account service timed out. Please try again."
        : "Unable to reach the account service. Check your connection and try again.",
    );
  }
  // Edge/CDN errors can return HTML; never surface a JSON parse error to the user.
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    const error = new Error(result?.error || "Account service unavailable.");
    error.status = response.status;
    throw error;
  }
  if (!result || typeof result !== "object" || Array.isArray(result))
    throw new Error("Account service returned an invalid response. Please try again.");
  const needsAcknowledgement = method !== "GET" &&
    ["signup/credentials", "envelope", "password/change", "email/complete", "logout", "logout-all"].includes(path);
  if (needsAcknowledgement && result.ok !== true)
    throw new Error("Account service returned an invalid response. Please try again.");
  if (result.parameters && result.csrf) {
    validateParameters(result.parameters);
    current = result;
  }
  return result;
}
export function runCrypto(payload) {
  return new Promise((resolve, reject) => {
    const worker = new Worker(
      new URL("./password-crypto-worker.js", import.meta.url),
      { type: "module" },
    );
    const timeout = setTimeout(() => {
      worker.terminate();
      reject(new Error("Password processing timed out. Please try again."));
    }, 60000);
    const finish = () => {
      clearTimeout(timeout);
      worker.terminate();
    };
    worker.onmessage = ({ data }) => {
      finish();
      data.error ? reject(new Error(data.error)) : resolve(data.result);
    };
    worker.onerror = () => {
      finish();
      reject(
        new Error(
          "Unable to start password encryption. Please update your browser.",
        ),
      );
    };
    worker.postMessage(payload);
  });
}
export async function unlock(email, password) {
  const parameters = await request("login/parameters", { email });
  const { authSecret } = await runCrypto({
    operation: "derive",
    password,
    parameters,
  });
  const record = await request("login", { email, authSecret });
  // Parameter substitution/version races never cause us to decrypt under a different identity.
  if (JSON.stringify(record.parameters) !== JSON.stringify(parameters))
    throw new Error("Account changed. Please sign in again.");
  const result = record.envelope
    ? await runCrypto({
        operation: "decrypt",
        password,
        parameters,
        envelope: record.envelope,
      })
    : null;
  return { ...record, apiKey: result?.apiKey, authSecret };
}
export function signOutService() {
  if (signingOut) return signingOut;
  signingOut = (async () => {
    try {
      if (!current) await send("session", undefined, "GET");
      await send("logout", {});
    } catch {
      /* Local signout must always complete. */
    } finally {
      clearAuthSession();
    }
  })().finally(() => {
    signingOut = null;
  });
  return signingOut;
}
export async function validateNewPassword(password) {
  const length = Array.from(password).length;
  if (length < 15 || new TextEncoder().encode(password).length > 1024)
    throw new Error("Use at least 15 characters (up to 1,024 UTF-8 bytes).");
  // Local common-password screening: no passwords or password hashes leave the browser.
  if (
    /^(password|letmein|qwerty|123456|abcdef|iloveyou|welcome|admin)[\d!@#$ ._-]*$/i.test(
      password,
    ) ||
    /^(.)\1+$/.test(password)
  )
    throw new Error(
      "Choose a less common password or use your password manager to generate one.",
    );
}
