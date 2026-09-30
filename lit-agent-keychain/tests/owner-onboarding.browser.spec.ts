import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
const skill = await readFile(new URL("../SKILL.md", import.meta.url), "utf8");
const packageVersion = /^version: (.+)$/m.exec(skill)![1];
const key = "ab".repeat(32);
test.beforeEach(async ({ page }) => {
  await page.goto("/fixtures/owner-onboarding.html");
});
test("selective approval and live session connection at desktop and mobile sizes", async ({
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
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "Connect to a session", exact: true })
    .click();
  const panel = page.getByRole("region", {
    name: "Connect Muse test to a session",
  });
  await expect(panel).toContainText(key);
  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async (text: string) => {
          (window as any).copied = text;
        },
      },
    });
  });
  for (const label of [
    "Agent prompt",
    "Claude Code",
    "Codex",
    "Cursor / Windsurf",
    "SDK",
  ]) {
    await panel
      .getByRole("button", { name: `Copy ${label}`, exact: true })
      .click();
    await expect(panel.getByRole("status")).toHaveText(`Copied ${label}`);
    const copied = await page.evaluate(() => (window as any).copied);
    expect(copied).toContain(`@lit-protocol/keychain@${packageVersion}`);
    expect(copied).toContain("http://127.0.0.1:55449");
    expect(copied).not.toContain("fixture-execution-key");
    expect(copied).not.toContain(".keychain.json");
    if (label === "Agent prompt") {
      expect(copied).toContain('"Muse test"');
      expect(copied).toContain(key);
      expect(copied).toContain("https://keychain.litprotocol.com/SKILL.md");
      expect(copied).toContain("verify its public key matches");
    } else if (label === "Cursor / Windsurf") {
      const server = JSON.parse(copied).mcpServers["lit-keychain"];
      expect(server.args).toEqual([
        "-y",
        `@lit-protocol/keychain@${packageVersion}`,
        "mcp",
        "/absolute/path/agent-identity.json",
      ]);
      expect(server.env.KEYCHAIN_SERVICE_URL).toBe("http://127.0.0.1:55449");
    }
  }
  for (const width of [320, 390, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async () => {
          throw new Error("Clipboard denied");
        },
      },
    });
  });
  await panel
    .getByRole("button", { name: "Copy Agent prompt", exact: true })
    .click();
  await expect(panel.getByRole("status")).toContainText(
    "Select and copy the text below",
  );
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
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "Connect to a session", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Connect Existing agent to a session" }),
  ).toContainText(key);
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
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "Connect to a session", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Connect Existing agent to a session" }),
  ).toContainText(key);
  await expect(
    page.getByRole("button", { name: "+ Grant secrets", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Secrets", exact: true }).click();
  await page.getByRole("button", { name: /ONE.*1 agent/ }).click();
  await expect(
    page.getByRole("button", {
      name: /^(Download agent config|Agent config)$/,
    }),
  ).toHaveCount(0);
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
  // A known key keeps its name: agents are identified by key, never by label.
  await page.getByLabel("Agent public key", { exact: true }).fill(key);
  const nameField = page.getByLabel("Agent name", { exact: true });
  await expect(nameField).toHaveValue("Existing agent");
  await expect(nameField).toHaveAttribute("readonly", "");
  await expect(
    page.getByText(/This key is already approved as Existing agent/),
  ).toBeVisible();
  await page.getByLabel("Agent public key", { exact: true }).fill("");
  await expect(nameField).not.toHaveAttribute("readonly", "");
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
