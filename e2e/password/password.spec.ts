import { test, expect } from "@playwright/test";
import { mkdir } from "node:fs/promises";
const password = "a long password from my manager",
  key = Buffer.alloc(32, 8).toString("base64");
let creates = 0;
test.beforeEach(async ({ page }) => {
  await page.route("http://localhost:8000/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    let result: unknown = [];
    if (path.endsWith("/new_account")) {
      creates++;
      result = { api_key: key, wallet_address: "0x" + "34".repeat(20) };
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
  await page.locator("#password-create-password").fill(password);
  await page.locator("#password-create-submit").click();
  await expect(page.locator("#login-status")).toContainText(
    "temporarily unavailable",
  );
  await expect(page.locator("body")).not.toHaveClass(/has-api-key/);
  expect(creates).toBe(1);
  await page.locator("#password-create-password").fill(password);
  await page.locator("#password-create-submit").click();
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
  await page.locator("#password-login-password").fill(password);
  await page.locator("#password-login-form button[type=submit]").click();
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
  await page.locator("#password-change-form button").click();
  await expect(page.locator("body")).not.toHaveClass(/has-api-key/);
  await expect(page.locator("#login-status")).toContainText("Password changed");
  await page
    .locator("#password-login-password")
    .fill("my newly generated password phrase");
  await page.locator("#password-login-form button[type=submit]").click();
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
  await page.locator("#password-login-form button[type=submit]").click();
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
