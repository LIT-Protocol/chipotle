import {
  enabled,
  request,
  runCrypto,
  unlock,
  signOutService,
  validateNewPassword,
  clearAuthSession,
  hasAuthSession,
} from "./password-client.js";
import {
  setMode,
  setApiKey,
  getApiKey,
  getClient,
  logOut,
  isAuthenticated,
  clearChainSecuredSession,
  setUsageKeyOverride,
} from "./auth.js";
import { showStatus } from "./ui-utils.js";
const MARKER = "chipotle_password_identity";
const PENDING = "chipotle_password_pending";
const NOTICE = "chipotle_password_notice";
const $ = (id) => document.getElementById(id);
let verified = null;
const help =
  "Save this password in your password manager. There’s no “Forgot password” option, and we can’t reset it for you.";
export function resetPasswordIdentity() {
  // API-key and wallet sign-outs call this too; only contact the auth service
  // when this tab actually held a password identity or storage session.
  const hadIdentity = sessionStorage.getItem(MARKER) !== null;
  sessionStorage.removeItem(MARKER);
  const button = $("password-settings-open");
  if (button) button.hidden = true;
  if (hadIdentity || hasAuthSession()) void signOutService();
}
function enter(record, apiKey) {
  verified = null;
  sessionStorage.setItem(
    MARKER,
    JSON.stringify({ id: record.parameters.id, email: record.email }),
  );
  sessionStorage.removeItem(PENDING);
  clearChainSecuredSession();
  setUsageKeyOverride("");
  setMode("api");
  setApiKey(apiKey);
  $("password-settings-open").hidden = false;
}
async function busy(form, fn, status = "login-status") {
  const buttons = [...form.querySelectorAll("button")];
  buttons.forEach((b) => (b.disabled = true));
  let navigate = false;
  try {
    navigate = (await fn()) === true;
    if (navigate) {
      // A real same-origin navigation signals successful submission to password
      // managers. Leave submitted fields intact until this document unloads;
      // never put passwords in the URL, storage, or a network form submission.
      window.location.replace(
        window.location.pathname + window.location.search,
      );
    }
  } catch (e) {
    showStatus(
      status,
      e.message || "Unable to complete this request.",
      "error",
    );
  } finally {
    buttons.forEach((b) => (b.disabled = false));
    if (!navigate)
      form
        .querySelectorAll('input[type="password"], input[data-password]')
        .forEach((i) => (i.value = ""));
  }
}
function showCreate(record) {
  // Retain only signup metadata, never the derived credential from unlock().
  verified = {
    parameters: record.parameters,
    email: record.email,
    state: record.state,
    operation: record.operation,
  };
  $("password-create-form").hidden = false;
  $("password-signup-form").hidden = true;
  $("password-signup-email").value = record.email;
  $("password-create-email").value = record.email;
  $("password-create-submit").textContent =
    record.state === "creating" ? "Finish saving account" : "Create account";
}
async function createAccount(password) {
  await validateNewPassword(password);
  let record = verified;
  if (!record) throw new Error("Verify your email first.");
  const { authSecret } = await runCrypto({
    operation: "derive",
    password,
    parameters: record.parameters,
  });
  if (record.state === "verified") {
    await request("signup/credentials", { authSecret });
    record = await request("session", undefined, "GET");
    verified = record;
  } else {
    // Resume only after proving the chosen password, never silently replacing it.
    record = await request("login", { email: record.email, authSecret });
    verified = record;
  }
  let pending;
  try {
    pending = JSON.parse(sessionStorage.getItem(PENDING) || "null");
  } catch {
    pending = null;
  }
  if (pending && pending.id !== record.parameters.id)
    throw new Error(
      "Another signup is unfinished in this tab. Save its API key before switching accounts.",
    );
  if (record.state === "active") {
    const { apiKey } = await runCrypto({
      operation: "decrypt",
      password,
      parameters: record.parameters,
      envelope: record.envelope,
    });
    enter(record, apiKey);
    return true;
  }
  if (record.state === "reserved") {
    if (pending)
      throw new Error("An account is already pending. Please sign in again.");
    const claimed = await request("signup/begin", { authSecret });
    // Mark the attempt before calling the non-idempotent legacy API. Never auto-retry.
    pending = {
      id: record.parameters.id,
      operation: claimed.operation,
      uncertain: true,
    };
    sessionStorage.setItem(PENDING, JSON.stringify(pending));
    setMode("api");
    const client = await getClient();
    const result = await client.newAccount({
      accountName:
        $("password-account-name").value.trim() || record.email.split("@")[0],
      accountDescription: "",
      email: record.email,
    });
    pending = {
      ...pending,
      apiKey: result.api_key,
      account: result.wallet_address,
      uncertain: false,
    };
    sessionStorage.setItem(PENDING, JSON.stringify(pending));
  }
  if (!pending?.apiKey)
    throw new Error(
      "Account creation may have completed, but this tab has no returned key. Do not create another account automatically. An email or password cannot recover that missing key.",
    );
  // Preserve the returned key in tab storage before the upload, but do not enter
  // the dashboard until encrypted storage has acknowledged it.
  $("password-backup").hidden = false;
  $("password-backup-key").value = pending.apiKey;
  const { envelope } = await runCrypto({
    operation: "encrypt",
    password,
    parameters: record.parameters,
    apiKey: pending.apiKey,
    account: pending.account,
  });
  // Keep identical ciphertext for retry after an acknowledgement was lost.
  if (!pending.envelope) {
    pending.envelope = envelope;
    sessionStorage.setItem(PENDING, JSON.stringify(pending));
  }
  await request(
    "envelope",
    { authSecret, operation: pending.operation, envelope: pending.envelope },
    "PUT",
  );
  $("password-backup-key").value = "";
  $("password-backup").hidden = true;
  enter(record, pending.apiKey);
  // The success navigation reloads the page; show the reminder after it.
  sessionStorage.setItem(NOTICE, help);
  return true;
}
export function initPasswordLogin() {
  if (!enabled()) return;
  const notice = sessionStorage.getItem(NOTICE);
  if (notice) {
    sessionStorage.removeItem(NOTICE);
    // dashboard-status is cleared by the table preload, so use the overview
    // status area once signed in.
    showStatus(
      isAuthenticated() ? "overview-status" : "login-status",
      notice,
      isAuthenticated() ? "info" : "success",
    );
  }
  const marker = () => {
    try {
      return JSON.parse(sessionStorage.getItem(MARKER) || "null");
    } catch {
      return null;
    }
  };
  $("password-settings-open").hidden = !marker() || !getApiKey();
  $("password-login-form").addEventListener("submit", (event) => {
    event.preventDefault();
    void busy(event.currentTarget, async () => {
      const password = $("password-login-password").value;
      const record = await unlock($("password-login-email").value, password);
      if (record.apiKey) {
        enter(record, record.apiKey);
        return true;
      }
      showCreate(record);
      $("login-tab-new").click();
      showStatus(
        "login-status",
        "Finish creating and saving your account with the password you chose.",
        "info",
      );
    });
  });
  $("password-signup-form").addEventListener("submit", (event) => {
    event.preventDefault();
    void busy(event.currentTarget, async () => {
      const result = await request("signup/start", {
        email: $("password-signup-email").value,
      });
      showStatus("login-status", result.message, "info");
    });
  });
  $("password-create-form").addEventListener("submit", (event) => {
    event.preventDefault();
    void busy(event.currentTarget, () =>
      createAccount($("password-create-password").value),
    );
  });
  document.querySelectorAll("[data-show-password]").forEach((button) =>
    button.addEventListener("click", () => {
      const input = $(button.dataset.showPassword);
      input.dataset.password = "true";
      input.type = input.type === "password" ? "text" : "password";
      button.textContent =
        input.type === "password" ? "Show password" : "Hide password";
    }),
  );
  $("password-settings-open").addEventListener("click", () => {
    if ($("account-dropdown-trigger").getAttribute("aria-expanded") === "true")
      $("account-dropdown-trigger").click();
    $("password-settings").showModal();
    const email = marker()?.email || "";
    $("password-settings-email").value = email;
    $("password-change-username").value = email;
    $("password-email-username").value = email;
  });
  $("password-settings-close").addEventListener("click", () =>
    $("password-settings").close(),
  );
  $("password-settings").addEventListener("close", () => {
    $("password-settings")
      .querySelectorAll('input[type="password"]')
      .forEach((input) => (input.value = ""));
  });
  $("password-change-form").addEventListener("submit", (event) => {
    event.preventDefault();
    void busy(
      event.currentTarget,
      async () => {
        const identity = marker();
        if (!identity)
          throw new Error("Sign in with email and password first.");
        const oldPassword = $("password-current").value,
          newPassword = $("password-new").value;
        await validateNewPassword(newPassword);
        const record = await unlock(identity.email, oldPassword);
        if (
          record.parameters.id !== identity.id ||
          record.apiKey !== getApiKey()
        )
          throw new Error("Account changed. Sign out and sign in again.");
        const encrypted = await runCrypto({
          operation: "change",
          password: oldPassword,
          newPassword,
          parameters: record.parameters,
          envelope: record.envelope,
        });
        await request("password/change", {
          oldAuthSecret: encrypted.oldAuthSecret,
          authSecret: encrypted.authSecret,
          envelope: encrypted.envelope,
        });
        clearAuthSession();
        logOut();
        sessionStorage.setItem(
          NOTICE,
          "Password changed. Save it in your password manager, then sign in again.",
        );
        return true;
      },
      "password-settings-status",
    );
  });
  $("password-email-form").addEventListener("submit", (event) => {
    event.preventDefault();
    void busy(
      event.currentTarget,
      async () => {
        const identity = marker();
        if (!identity)
          throw new Error("Sign in with email and password first.");
        const record = await unlock(
          identity.email,
          $("password-email-current").value,
        );
        if (
          record.parameters.id !== identity.id ||
          record.apiKey !== getApiKey()
        )
          throw new Error("Account changed. Sign out and sign in again.");
        const result = await request("email/start", {
          email: $("password-email-new").value,
          authSecret: record.authSecret,
        });
        showStatus("password-settings-status", result.message, "info");
      },
      "password-settings-status",
    );
  });
  function readVerificationLink() {
    const fragment = new URLSearchParams(location.hash.slice(1));
    const proof = fragment.get("verify"),
      purpose = fragment.get("purpose");
    if (proof) {
      if (isAuthenticated()) logOut();
      history.replaceState(null, "", location.pathname + location.search);
      $("password-verify-panel").hidden = false;
      $("password-verify-copy").textContent =
        purpose === "email"
          ? "Sign in with your current email and password to confirm your new email address."
          : "Confirm your email to create a new account.";
      $("password-email-confirm-fields").hidden = purpose !== "email";
      $("login-auth-mode-password").click();
      $("password-verify-form").onsubmit = (event) => {
        event.preventDefault();
        void busy(event.currentTarget, async () => {
          if (purpose === "email") {
            const password = $("password-verify-password").value;
            const record = await unlock(
              $("password-verify-email").value,
              password,
            );
            await request("email/complete", {
              token: proof,
              authSecret: record.authSecret,
            });
            logOut();
            showStatus(
              "login-status",
              "Email updated. Sign in with your new email and existing password.",
              "success",
            );
          } else {
            const record = await request("signup/verify", { token: proof });
            showCreate(record);
            $("login-tab-new").click();
            showStatus(
              "login-status",
              "Email verified. Choose a password to create your account.",
              "success",
            );
          }
          $("password-verify-panel").hidden = true;
        });
      };
    }
  }
  readVerificationLink();
  window.addEventListener("hashchange", readVerificationLink);
}
