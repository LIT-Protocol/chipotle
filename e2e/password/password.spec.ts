import { test, expect, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
const password = "a long password from my manager",
  key = Buffer.alloc(32, 8).toString("base64");
let creates = 0;
let rejectNextCreate = false;
async function submitAndNavigate(page: Page, selector: string) {
  await Promise.all([page.waitForNavigation(), page.locator(selector).click()]);
}
// Model managers which set DOM values without keyboard/input events. This tests
// the site's autofill contract, not any vendor's browser-chrome save prompt.
async function autofill(page: Page, values: Record<string, string>) {
  await page.evaluate(values => {
    for (const [id, value] of Object.entries(values))
      (document.getElementById(id) as HTMLInputElement).value = value;
  }, values);
}
test.beforeEach(async ({ page }) => {
  await page.route("http://localhost:8000/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    let result: unknown = [];
    if (path.endsWith("/new_account")) {
      creates++;
      if (rejectNextCreate) {
        rejectNextCreate = false;
        await route.fulfill({
          status: 402,
          json: { error: "Payment required: account limit reached" },
        });
        return;
      }
      // Account bindings are UNIQUE in the auth database: give each created
      // account its own wallet address, as the real API does.
      result = {
        api_key: key,
        wallet_address: "0x" + "34".repeat(18) + creates.toString(16).padStart(4, "0"),
      };
    } else if (path.endsWith("/account_exists")) result = true;
    else if (path.includes("billing"))
      result = { enabled: false, billing_enabled: false };
    else if (path.includes("wallet_address")) result = "0x" + "34".repeat(20);
    await route.fulfill({ json: result });
  });
  await page.route("http://localhost:8001/**", (route) =>
    route.fulfill({ status: 503, json: { error: "Disabled in browser test" } }),
  );
});
test("new password account verifies email, encrypts locally, reloads, signs in and changes password", async ({
  page,
  request,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("./");
  await expect(page.locator("#login-auth-mode-password")).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await page.locator("#login-tab-new").click();
  await page.locator("#password-signup-email").fill("browser@example.com");
  await page.locator("#password-signup-form button").click();
  await expect(page.locator("#login-status")).toContainText(
    "verification link",
  );
  let link = "";
  await expect
    .poll(async () => {
      const messages = await (
        await request.get("http://localhost:8080/__test/mail")
      ).json();
      link =
        messages
          .find((m: any) => m.to === "browser@example.com")
          ?.text.match(/http:\/\/localhost:8080\/\S+/)?.[0] || "";
      return !!link;
    })
    .toBe(true);
  await page.goto(link);
  await expect(page).not.toHaveURL(/verify=/);
  await page.locator("#password-verify-form button").click();
  await expect(page.locator("#password-create-form")).toBeVisible();
  await expect(page.locator("#password-save-notice")).toContainText(
    "password manager",
  );
  let failUpload = true;
  await page.route("http://localhost:8787/auth/v1/envelope", async (route) => {
    if (failUpload && route.request().method() === "PUT") {
      failUpload = false;
      await route.fulfill({
        status: 503,
        json: { error: "Account storage temporarily unavailable." },
      });
    } else await route.continue();
  });
  await expect(page.locator("#password-create-form")).toHaveAttribute("method", "post");
  await expect(page.locator("#password-create-email")).toHaveAttribute("autocomplete", "username");
  await expect(page.locator("#password-create-password")).toHaveAttribute("autocomplete", "new-password");
  await expect(page.locator("#password-create-password")).toHaveAttribute("passwordrules", /minlength: 15/);
  await autofill(page, { "password-create-password": password });
  await page.locator("#password-create-submit").click();
  await expect(page.locator("#login-status")).toContainText(
    "temporarily unavailable",
  );
  await expect(page.locator("body")).not.toHaveClass(/has-api-key/);
  expect(creates).toBe(1);
  await autofill(page, { "password-create-password": password });
  expect(await page.locator("#password-create-form").evaluate(form =>
    Object.fromEntries(new FormData(form as HTMLFormElement)))).toMatchObject({
      username: "browser@example.com", "new-password": password,
    });
  for (const body of ["<html>Upstream unavailable</html>", "{}", "null"]) {
    await page.route("http://localhost:8787/auth/v1/envelope", route => route.fulfill({ status: 200, body }));
    await autofill(page, { "password-create-password": password });
    await page.locator("#password-create-submit").click();
    await expect(page.locator("#login-status")).toContainText(/invalid response|did not confirm/);
    await expect(page.locator("body")).not.toHaveClass(/has-api-key/);
    expect(creates).toBe(1);
  }
  await page.unroute("http://localhost:8787/auth/v1/envelope");
  await autofill(page, { "password-create-password": password });
  await submitAndNavigate(page, "#password-create-submit");
  await expect(page.locator("body")).toHaveClass(/has-api-key/);
  expect(creates).toBe(1);
  expect(
    await page.evaluate(() => sessionStorage.getItem("accountconfig_api_key")),
  ).toBe(key);
  expect(
    await page.evaluate(() => JSON.stringify({ ...sessionStorage })),
  ).not.toContain(password);
  const records = await (
    await request.get("http://localhost:8080/__test/records")
  ).json();
  expect(JSON.stringify(records)).not.toContain(key);
  expect(JSON.stringify(records)).not.toContain(password);
  expect(records[0].state).toBe("active");
  await page.reload();
  await expect(page.locator("body")).toHaveClass(/has-api-key/);
  await page.locator("#account-dropdown-trigger").click();
  await page.locator("#account-signout-btn").click();
  await expect(page.locator("body")).not.toHaveClass(/has-api-key/);
  await page.locator("#password-login-email").fill("browser@example.com");
  await page
    .locator("#password-login-password")
    .fill("a wrong password that is long");
  await page.locator("#password-login-form button[type=submit]").click();
  await expect(page.locator("#login-status")).toContainText("incorrect");
  await autofill(page, { "password-login-email": "browser@example.com", "password-login-password": password });
  await expect(page.locator("#password-login-password")).toHaveAttribute("autocomplete", "current-password");
  // Enter submits the same real form that a password manager's submit button uses.
  await Promise.all([page.waitForNavigation(), page.locator("#password-login-password").press("Enter")]);
  await expect(page.locator("body")).toHaveClass(/has-api-key/);
  await page.locator("#account-dropdown-trigger").click();
  await page.locator("#password-settings-open").click();
  await expect(page.locator("#password-settings")).toBeVisible();
  await mkdir("../.context", { recursive: true });
  await page.screenshot({ path: "../.context/password-settings.png" });
  await page.locator("#password-current").fill(password);
  await page
    .locator("#password-new")
    .fill("my newly generated password phrase");
  await expect(page.locator("#password-change-username")).toHaveValue("browser@example.com");
  await submitAndNavigate(page, "#password-change-form button");
  await expect(page.locator("body")).not.toHaveClass(/has-api-key/);
  await expect(page.locator("#login-status")).toContainText("Password changed");
  await page.locator("#password-login-email").fill("browser@example.com");
  await page
    .locator("#password-login-password")
    .fill("my newly generated password phrase");
  await submitAndNavigate(page, "#password-login-form button[type=submit]");
  await expect(page.locator("body")).toHaveClass(/has-api-key/);
  await page.locator("#account-dropdown-trigger").click();
  await page.locator("#password-settings-open").click();
  await page.locator("#password-email-new").fill("changed-browser@example.com");
  await page
    .locator("#password-email-current")
    .fill("my newly generated password phrase");
  await page.locator("#password-email-form button").click();
  await expect(page.locator("#password-settings-status")).toContainText(
    "verification link",
  );
  let emailLink = "";
  await expect
    .poll(async () => {
      const messages = await (
        await request.get("http://localhost:8080/__test/mail")
      ).json();
      emailLink =
        messages
          .find((m: any) => m.to === "changed-browser@example.com")
          ?.text.match(/http:\/\/localhost:8080\/\S+/)?.[0] || "";
      return !!emailLink;
    })
    .toBe(true);
  await page.locator("#password-settings-close").click();
  await page.goto(emailLink);
  await page.locator("#password-verify-email").fill("browser@example.com");
  await page
    .locator("#password-verify-password")
    .fill("my newly generated password phrase");
  await page.locator("#password-verify-form button").click();
  await expect(page.locator("#login-status")).toContainText("Email updated");
  await page
    .locator("#password-login-email")
    .fill("changed-browser@example.com");
  await page
    .locator("#password-login-password")
    .fill("my newly generated password phrase");
  await submitAndNavigate(page, "#password-login-form button[type=submit]");
  await expect(page.locator("body")).toHaveClass(/has-api-key/);
  expect(errors).toEqual([]);
});
test("legacy API-key and wallet choices stay available with no migration prompt", async ({
  page,
}) => {
  await page.goto("./");
  await page.locator("#login-auth-mode-api").click();
  await page.locator("#login-api-key").fill(key);
  await page.locator("#btn-login").click();
  await expect(page.locator("body")).toHaveClass(/has-api-key/);
  await expect(page.locator("#password-settings-open")).toBeHidden();
  await page.locator("#account-dropdown-trigger").click();
  await page.locator("#account-signout-btn").click();
  await page.locator("#login-auth-mode-chainsecured").click();
  await expect(page.locator("#btn-login-wallet")).toBeVisible();
  await page.locator("#login-auth-mode-password").click();
  await mkdir("../.context", { recursive: true });
  await page.screenshot({ path: "../.context/password-login.png" });
  await expect(page.locator("#password-login-form")).toBeVisible();
});

test("mobile signup explains password storage without a recovery link", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("./");
  await page.locator("#login-tab-new").click();
  await expect(page.locator("#password-signup-form")).toBeVisible();
  await expect(
    page.getByRole("link", { name: /forgot password/i }),
  ).toHaveCount(0);
  await page.locator("#password-signup-email").scrollIntoViewIfNeeded();
  await mkdir("../.context", { recursive: true });
  await page.screenshot({
    path: "../.context/password-mobile.png",
    fullPage: true,
  });
});

test("unconfigured deployments retain the original API-key and wallet login", async ({
  page,
}) => {
  await page.route("**/password-client.js", async (route) => {
    const response = await route.fetch();
    const source = (await response.text()).replace(
      '"__LIT_AUTH_BASE_URL__"',
      '""',
    );
    await route.fulfill({ response, body: source });
  });
  await page.goto("./");
  await expect(page.locator("#login-auth-mode-password")).toBeHidden();
  await expect(page.locator("#login-auth-mode-api")).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(page.locator("#btn-login")).toBeVisible();
  await expect(page.locator("#password-login-form")).toBeHidden();
});

test("password is the default even after wallet use, with accessible access explanations", async ({ page }) => {
  await page.addInitScript(() => sessionStorage.setItem("accountconfig_mode", "sovereign"));
  await page.goto("./");
  const passwordChoice = page.locator("#login-auth-mode-password");
  await expect(passwordChoice).toHaveAttribute("aria-checked", "true");
  await expect(page.locator("#password-login-form")).toBeVisible();
  for (const [mode, explanation] of [
    ["password", "there is no password reset"],
    ["api", "Existing API-key accounts cannot switch"],
    ["chainsecured", "Your wallet controls account permissions"],
  ]) {
    const choice = page.locator("#login-auth-mode-" + mode);
    await choice.hover();
    await expect(page.locator("#login-help-" + mode)).toBeVisible();
    await expect(choice).toHaveAccessibleDescription(new RegExp(explanation));
  }
  await page.mouse.move(0, 0);
  await passwordChoice.focus();
  await expect(page.locator("#login-help-password")).toBeVisible();
  await mkdir("../.context", { recursive: true });
  await page.screenshot({ path: "../.context/password-login-tooltip.png" });
  await page.keyboard.press("Escape");
  await expect(page.locator("#login-help-password")).toBeHidden();
});

test("a definitive Lit API rejection releases the creation claim so the same tab can retry", async ({
  page,
  request,
}) => {
  creates = 0;
  await page.goto("./");
  await page.locator("#login-tab-new").click();
  await page.locator("#password-signup-email").fill("retry@example.com");
  await page.locator("#password-signup-form button").click();
  let link = "";
  await expect
    .poll(async () => {
      const messages = await (
        await request.get("http://localhost:8080/__test/mail")
      ).json();
      link =
        messages
          .find((m: any) => m.to === "retry@example.com")
          ?.text.match(/http:\/\/localhost:8080\/\S+/)?.[0] || "";
      return !!link;
    })
    .toBe(true);
  await page.goto(link);
  await page.locator("#password-verify-form button").click();
  await expect(page.locator("#password-create-form")).toBeVisible();
  rejectNextCreate = true;
  await page.locator("#password-create-password").fill(password);
  await page.locator("#password-create-submit").click();
  await expect(page.locator("#login-status")).toContainText("rejected");
  await expect(page.locator("#login-status")).toContainText("try again");
  await expect(page.locator("body")).not.toHaveClass(/has-api-key/);
  expect(creates).toBe(1);
  const records = async () =>
    (await (await request.get("http://localhost:8080/__test/records")).json())
      .find((r: any) => r.email === "retry@example.com");
  expect((await records()).state).toBe("creating");
  // Same tab, password re-entered: the claim is released and creation retried.
  await page.locator("#password-create-password").fill(password);
  await submitAndNavigate(page, "#password-create-submit");
  await expect(page.locator("body")).toHaveClass(/has-api-key/);
  expect(creates).toBe(2);
  expect((await records()).state).toBe("active");
  expect(
    await page.evaluate(() => sessionStorage.getItem("accountconfig_api_key")),
  ).toBe(key);
});

for (const status of [408, 499, 503, "network"]) {
  test(`ambiguous creation failure ${status} cannot create a second account`, async ({ page, request }) => {
    const email = `uncertain-${status}@example.com`;
    let attempts = 0;
    await page.route("http://localhost:8000/core/v1/new_account", async route => {
      attempts++;
      if (status === "network") await route.abort("failed");
      else await route.fulfill({ status: Number(status), json: { error: "Ambiguous upstream result" } });
    });
    await page.goto("./");
    await page.locator("#login-tab-new").click();
    await page.locator("#password-signup-email").fill(email);
    await page.locator("#password-signup-form button").click();
    let link = "";
    await expect.poll(async () => {
      const mail = await (await request.get("http://localhost:8080/__test/mail")).json();
      link = mail.find((item: any) => item.to === email)?.text.match(/http:\/\/localhost:8080\/\S+/)?.[0] || "";
      return !!link;
    }).toBe(true);
    await page.goto(link);
    await page.locator("#password-verify-form button").click();
    await page.locator("#password-create-password").fill(password);
    await page.locator("#password-create-submit").click();
    await expect(page.locator("#password-create-submit")).toBeEnabled();
    expect(attempts).toBe(1);
    await page.locator("#password-create-password").fill(password);
    await page.locator("#password-create-submit").click();
    await expect(page.locator("#login-status")).toContainText("Account creation may have completed");
    expect(attempts).toBe(1);
    await expect(page.locator("body")).not.toHaveClass(/has-api-key/);
  });
}
