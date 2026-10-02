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
  navigateLogin,
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
// Passwords can’t be reset, so a typo in a new password would lock the user
// out. Require the same value twice before deriving anything from it. On a
// mismatch keep the first entry (it may be an unsaved generated password) and
// send the user back to the confirmation field.
function matching(passwordId, confirmId, status) {
  const confirm = $(confirmId);
  if ($(passwordId).value === confirm.value) return true;
  showStatus(
    status,
    "Passwords don’t match. Re-enter the confirmation so it matches the password above.",
    "error",
  );
  confirm.value = "";
  confirm.focus();
  return false;
}
// Re-mask inputs revealed with "Show passwords" and reset their toggles.
function conceal(container) {
  container.querySelectorAll("input[data-password]").forEach((input) => {
    input.type = "password";
    delete input.dataset.password;
  });
  container.querySelectorAll("[data-show-password]").forEach((button) => {
    button.textContent = button.dataset.showPassword.includes(" ")
      ? "Show passwords"
      : "Show password";
  });
}
// Forms currently awaiting a response. A verification link opened mid-submit
// would swap the auth session under the in-flight request, so it waits.
let inFlight = 0;
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
  inFlight++;
  let navigate = false;
  try {
    navigate = (await fn()) === true;
    if (navigate) {
      // A real same-origin navigation signals successful submission to password
      // managers. Leave submitted fields intact until this document unloads;
      // never put passwords in the URL, storage, or a network form submission.
      // Routing uses fragments, so changing the URL alone could be a same-
      // document navigation. Explicitly reload to complete the form lifecycle.
      history.replaceState(null, "", window.location.pathname + window.location.search + "#overview");
      window.location.reload();
    }
  } catch (e) {
    showStatus(
      status,
      e.message || "Unable to complete this request.",
      "error",
    );
  } finally {
    inFlight--;
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
  // A previous attempt in this tab was definitively rejected by the Lit API
  // (a known pre-creation rejection). Release that claim and try again.
  const rejected = record.state === "creating" && pending?.rejected === true;
  if (record.state === "reserved" || rejected) {
    if (pending && !rejected)
      throw new Error("An account is already pending. Please sign in again.");
    const claimed = await request(
      "signup/begin",
      rejected
        ? { authSecret, retry: true, operation: pending.operation }
        : { authSecret },
    );
    if (!/^[0-9a-f]{32}$/.test(claimed.operation || ""))
      throw new Error("Account service returned an invalid creation claim. Sign in again.");
    // Mark the attempt before calling the non-idempotent legacy API. Never
    // auto-retry an ambiguous (network/5xx) failure.
    pending = {
      id: record.parameters.id,
      operation: claimed.operation,
      uncertain: true,
    };
    sessionStorage.setItem(PENDING, JSON.stringify(pending));
    setMode("api");
    const client = await getClient();
    let result;
    try {
      result = await client.newAccount({
        accountName:
          $("password-account-name").value.trim() || record.email.split("@")[0],
        accountDescription: "",
        email: record.email,
      });
    } catch (e) {
      // Only known validation/access/rate-limit rejections permit retry. In
      // particular, 408/499 may come from a proxy after the upstream committed.
      // Unknown statuses, 5xx and network failures remain uncertain.
      if ([400, 401, 402, 403, 404, 405, 413, 415, 422, 429].includes(e?.status)) {
        pending = { ...pending, uncertain: false, rejected: true };
        sessionStorage.setItem(PENDING, JSON.stringify(pending));
        throw new Error(
          `Account creation was rejected (${e.message}). Nothing was created; you can try again.`,
        );
      }
      throw e;
    }
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
  const saved = await request(
    "envelope",
    { authSecret, operation: pending.operation, envelope: pending.envelope },
    "PUT",
  );
  if (saved.ok !== true)
    throw new Error("Account storage did not confirm the save. Please try again.");
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
      navigateLogin("#create-account");
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
    if (!matching("password-create-password", "password-create-confirm", "login-status"))
      return;
    void busy(event.currentTarget, () =>
      createAccount($("password-create-password").value),
    );
  });
  document.querySelectorAll("[data-show-password]").forEach((button) =>
    button.addEventListener("click", () => {
      // One toggle may reveal a password and its confirmation together.
      const inputs = button.dataset.showPassword.split(/\s+/).map($);
      const reveal = inputs[0].type === "password";
      inputs.forEach((input) => {
        input.dataset.password = "true";
        input.type = reveal ? "text" : "password";
      });
      const plural = inputs.length > 1 ? "passwords" : "password";
      button.textContent = reveal ? `Hide ${plural}` : `Show ${plural}`;
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
    const dialog = $("password-settings");
    dialog
      .querySelectorAll('input[type="password"], input[data-password]')
      .forEach((input) => (input.value = ""));
    conceal(dialog);
  });
  $("password-change-form").addEventListener("submit", (event) => {
    event.preventDefault();
    if (!matching("password-new", "password-new-confirm", "password-settings-status"))
      return;
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
    if (!proof) return;
    // Drop the proof from the URL before anything else so it never reaches
    // history or referrers. A link opened while another form is mid-request
    // is left unconsumed; reopening it after that request settles works.
    history.replaceState(null, "", location.pathname + location.search);
    if (inFlight > 0) {
      showStatus(
        "login-status",
        "Finish the step in progress, then open the link from your email again.",
        "error",
      );
      return;
    }
    if (isAuthenticated()) logOut();
    $("login-auth-mode-password").click();
    // Any earlier link's panel (and its token) must not outlive this one.
    const panel = $("password-verify-panel"),
      verifyForm = $("password-verify-form");
    panel.hidden = true;
    verifyForm.onsubmit = null;
    if (purpose === "email") {
      // Changing the email on an existing account still needs the current
      // password, so this one keeps an explicit confirmation form.
      $("password-verify-copy").textContent =
        "Sign in with your current email and password to confirm your new email address.";
      $("password-email-confirm-fields").hidden = false;
      $("password-verify-submit").textContent = "Confirm new email";
      panel.hidden = false;
      verifyForm.onsubmit = (event) => {
        event.preventDefault();
        void busy(event.currentTarget, async () => {
          const record = await unlock(
            $("password-verify-email").value,
            $("password-verify-password").value,
          );
          await request("email/complete", {
            token: proof,
            authSecret: record.authSecret,
          });
          logOut();
          panel.hidden = true;
          showStatus(
            "login-status",
            "Email updated. Sign in with your new email and existing password.",
            "success",
          );
        });
      };
      return;
    }
    // Signup links confirm the email as soon as they open: the fragment is a
    // single-use proof and there is nothing for the user to decide until the
    // password step. Anything that runs this page's JavaScript with the link
    // (not plain prefetchers, which never see fragments) consumes it; the
    // recipient then simply requests a new link.
    navigateLogin("#create-account");
    const confirmSignup = async () => {
      panel.hidden = true;
      showStatus("login-status", "Confirming your email…", "info");
      try {
        const record = await request("signup/verify", { token: proof });
        showCreate(record);
        showStatus(
          "login-status",
          "Email confirmed. Choose a password to finish creating your account.",
          "success",
        );
        $("password-create-password").focus();
      } catch (e) {
        showStatus(
          "login-status",
          e.message || "Unable to confirm this email. Request a new link.",
          "error",
        );
        // The URL no longer carries the proof, so a connectivity or service
        // failure keeps it in memory behind a retry. A definitive rejection
        // (expired or already used) has nothing to retry.
        if (!e.status || e.status >= 500) {
          $("password-verify-copy").textContent =
            "Your email isn’t confirmed yet. Try again once you’re back online.";
          $("password-email-confirm-fields").hidden = true;
          $("password-verify-submit").textContent = "Try again";
          panel.hidden = false;
          verifyForm.onsubmit = (event) => {
            event.preventDefault();
            void confirmSignup();
          };
        }
      }
    };
    void confirmSignup();
  }
  readVerificationLink();
  window.addEventListener("hashchange", readVerificationLink);
}
