import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
const key = "ab".repeat(32);
test.beforeEach(async ({ page }) => {
  await page.goto("/fixtures/owner-onboarding.html");
});
test("selective approval and private config handoff at desktop and mobile sizes", async ({
  page,
}) => {
  await expect(page.getByRole("heading", { name: "Add agent" })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Approve selected secrets" }),
  ).toBeDisabled();
  await page.getByLabel("Agent name", { exact: true }).fill("Muse test");
  await page.getByLabel("Agent public key", { exact: true }).fill(key);
  await expect(page.getByRole("checkbox", { name: /ONE/ })).not.toBeChecked();
  await expect(page.getByRole("checkbox", { name: /TWO/ })).not.toBeChecked();
  await expect(page.getByRole("checkbox", { name: /DISABLED/ })).toBeDisabled();
  await expect(page.getByRole("checkbox", { name: /EXPIRED/ })).toBeDisabled();
  for (const width of [320, 390, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  await page.getByRole("checkbox", { name: /TWO/ }).check();
  await page.getByRole("button", { name: "Approve selected secrets" }).click();
  await expect(
    page.getByText("1 secret ready for this agent.", { exact: true }),
  ).toBeVisible();
  expect(await page.evaluate(() => (window as any).approvals)).toEqual(["TWO"]);
  await expect(
    page.getByRole("heading", { name: "3. Ready to use" }),
  ).toBeVisible();
  await expect(
    page.getByText(/No config download or agent restart is needed/),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Download agent config", exact: true }),
  ).not.toBeVisible();
  await page.getByText("Advanced: legacy static config").click();
  const download = page.waitForEvent("download");
  await page
    .getByRole("button", { name: "Download agent config", exact: true })
    .click();
  const file = await download;
  expect(file.suggestedFilename()).toBe("Muse-test.keychain.json");
  const config = JSON.parse(await readFile((await file.path())!, "utf8"));
  expect(Object.keys(config.secrets)).toEqual(["TWO"]);
  expect(config).not.toHaveProperty("privateKey");
  expect(JSON.stringify(config)).not.toContain("secret-value");
});
test("select all chooses eligible secrets without approving, and clear selection resets them", async ({
  page,
}) => {
  await page.getByLabel("Agent name", { exact: true }).fill("Agent");
  await page.getByLabel("Agent public key", { exact: true }).fill(key);
  await expect(
    page.getByRole("button", { name: "Clear selection", exact: true }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "Select all", exact: true }).click();
  await expect(page.getByRole("checkbox", { name: /ONE/ })).toBeChecked();
  await expect(page.getByRole("checkbox", { name: /TWO/ })).toBeChecked();
  await expect(
    page.getByRole("checkbox", { name: /DISABLED/ }),
  ).not.toBeChecked();
  await expect(
    page.getByRole("checkbox", { name: /EXPIRED/ }),
  ).not.toBeChecked();
  expect(await page.evaluate(() => (window as any).approvals)).toEqual([]);
  await page
    .getByRole("button", { name: "Clear selection", exact: true })
    .click();
  await expect(page.getByRole("checkbox", { name: /ONE/ })).not.toBeChecked();
  await expect(page.getByRole("checkbox", { name: /TWO/ })).not.toBeChecked();
  await expect(
    page.getByRole("button", { name: "Approve selected secrets" }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "Select all", exact: true }).click();
  await page.getByRole("checkbox", { name: /TWO/ }).uncheck();
  await page.getByRole("button", { name: "Approve selected secrets" }).click();
  await expect(
    page.getByText("1 secret ready for this agent.", { exact: true }),
  ).toBeVisible();
  expect(await page.evaluate(() => (window as any).approvals)).toEqual(["ONE"]);
  await expect(
    page.getByRole("button", { name: "Select all", exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "Clear selection", exact: true }),
  ).toBeDisabled();
});

test("validation blocks malformed input, cancellation writes nothing", async ({
  page,
}) => {
  await page.getByLabel("Agent name", { exact: true }).fill("Agent");
  await page.getByLabel("Agent public key", { exact: true }).fill("0x" + key);
  await page.getByRole("checkbox", { name: /ONE/ }).check();
  await page.getByRole("button", { name: "Approve selected secrets" }).click();
  expect(await page.evaluate(() => (window as any).approvals)).toEqual([]);
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByText("Closed onboarding")).toBeVisible();
});
test("partial denial retains completed approvals and retries only remaining work", async ({
  page,
}) => {
  await page.evaluate(() => {
    (window as any).failSecond = true;
  });
  await page.getByLabel("Agent name", { exact: true }).fill("Agent");
  await page.getByLabel("Agent public key", { exact: true }).fill(key);
  await page.getByRole("checkbox", { name: /ONE/ }).check();
  await page.getByRole("checkbox", { name: /TWO/ }).check();
  await page.getByRole("button", { name: "Approve selected secrets" }).click();
  await expect(page.getByRole("alert")).toContainText("Owner declined");
  await expect(
    page.getByText("1 secret ready for this agent.", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByLabel("Agent public key", { exact: true }),
  ).toBeDisabled();
  await page.evaluate(() => {
    (window as any).failSecond = false;
  });
  await page.getByRole("button", { name: "Retry remaining approvals" }).click();
  await expect(
    page.getByText("2 secrets ready for this agent.", { exact: true }),
  ).toBeVisible();
  expect(await page.evaluate(() => (window as any).approvals)).toEqual([
    "ONE",
    "TWO",
  ]);
});
test("owner can discover Add agent immediately after sign-in in the actual app", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.route("**/api/config", (route) =>
    route.fulfill({ json: { network: "test" } }),
  );
  await page.goto("/fixtures/owner-app.html");
  await page
    .getByRole("button", { name: "Use an existing passkey", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Secrets", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Agents", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Agents", exact: true }),
  ).toBeVisible();
  const add = page.getByRole("button", { name: "+ Add agent", exact: true });
  await expect(add).toBeVisible();
  await expect(page.getByText("Execution and account access")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Replace execution key" }),
  ).toHaveCount(0);
  for (const width of [320, 390, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  await add.click();
  await expect(page.getByLabel("Agent name", { exact: true })).toBeFocused();
  await page.getByLabel("Agent name", { exact: true }).fill("Existing agent");
  await page.getByLabel("Agent public key", { exact: true }).fill(key);
  await page.getByRole("checkbox", { name: /ONE/ }).check();
  await page.screenshot({
    path: "test-results/owner-onboarding-app-desktop.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: "test-results/owner-onboarding-app-mobile.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.getByRole("button", { name: "Approve selected secrets" }).click();
  await expect(
    page.getByText("1 secret ready for this agent.", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "3. Ready to use" }),
  ).toBeVisible();
  await expect(
    page.getByText(/No config download or agent restart is needed/),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Download agent config", exact: true }),
  ).not.toBeVisible();
  await page.getByText("Advanced: legacy static config").click();
  const download = page.waitForEvent("download");
  await page
    .getByRole("button", { name: "Download agent config", exact: true })
    .click();
  expect((await download).suggestedFilename()).toBe(
    "Existing-agent.keychain.json",
  );
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await expect(add).toBeFocused();
  // The Agents page inverts the Secrets listing: the agent row leads to its secrets.
  await page.getByRole("button", { name: /Existing agent.*1 secret/ }).click();
  await expect(
    page.getByRole("heading", { name: "Existing agent", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Accessible secrets", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".agent-row").getByText("ONE")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Revoke", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Download agent config", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "+ Grant secrets", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Secrets", exact: true }).click();
  await page.getByRole("button", { name: /ONE.*1 agent/ }).click();
  await expect(
    page.getByRole("button", { name: "Revoke", exact: true }),
  ).toBeVisible();
  // An agent approved elsewhere is offered as a checklist instead of asking
  // for its key again; it disappears once approved for this secret.
  await expect(
    page.getByRole("group", { name: "Approve your other agents" }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: /TWO/ }).click();
  const checklist = page.getByRole("group", {
    name: "Approve your other agents",
  });
  await expect(checklist).toBeVisible();
  const approveSelected = checklist.getByRole("button", {
    name: /Approve .*selected agent/,
  });
  await expect(approveSelected).toBeDisabled();
  await checklist.getByRole("button", { name: "Select all" }).click();
  await expect(
    checklist.getByRole("checkbox", { name: /Existing agent/ }),
  ).toBeChecked();
  await expect(approveSelected).toHaveText("Approve 1 selected agent");
  await approveSelected.click();
  await expect(
    page.getByRole("button", { name: /TWO.*1 agent/ }),
  ).toBeVisible();
  await expect(
    page.getByRole("group", { name: "Approve your other agents" }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Agents", exact: true }).click();
  await page.getByRole("button", { name: /Existing agent.*2 secrets/ }).click();
  await expect(
    page.getByRole("button", { name: "+ Grant secrets", exact: true }),
  ).toBeDisabled();
  expect(errors).toEqual([]);
});

test("empty vault explains next step without granting access", async ({
  page,
}) => {
  await page.goto("/fixtures/owner-onboarding.html?empty=1");
  await expect(
    page.getByText("Add a secret before approving an agent."),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Approve selected secrets" }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "Add secret", exact: true }).click();
  await expect(page.getByText("Adding secret")).toBeVisible();
});
