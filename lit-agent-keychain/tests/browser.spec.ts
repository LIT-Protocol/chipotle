import { test, expect } from "@playwright/test";
import { privateKeyToAccount } from "viem/accounts";
import { randomBytes, hex } from "../protocol/crypto.ts";
import { Keychain } from "../sdk/src/index.ts";
const mockLit = process.env.KEYCHAIN_TEST_LIT || "http://127.0.0.1:55440";
test.beforeEach(async ({ page }) => {
  await page.route("https://accounts.google.com/gsi/client", (route) =>
    route.fulfill({ contentType: "text/javascript", body: "" }),
  );
});
test("passkey onboarding encrypts locally, enrolls an agent, and revokes it", async ({
  page,
  context,
}) => {
  // Passkey onboarding, checkout, a stored secret and a connected service.
  test.setTimeout(240000);
  const cdp = await context.newCDPSession(page);
  await cdp.send("WebAuthn.enable");
  await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: "internal",
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  const requests: string[] = [];
  page.on("request", (r) => {
    if (r.method() === "POST" && r.url().includes("/api/"))
      requests.push(r.postData() || "");
  });
  await page.goto("/");
  await page.screenshot({
    path: "../.context/keychain/home-desktop.png",
    fullPage: true,
  });
  const mobile = await context.newPage();
  await mobile.setViewportSize({ width: 390, height: 844 });
  await mobile.goto("/");
  await expect(
    mobile.getByRole("button", { name: "Create a passkey", exact: true }),
  ).toBeVisible();
  // No page-wide horizontal overflow at phone width.
  expect(
    await mobile.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await mobile.screenshot({
    path: "../.context/keychain/home-mobile.png",
    fullPage: true,
  });
  await mobile.close();
  await page
    .getByRole("button", { name: "Create a passkey", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Secrets", exact: true }),
  ).toBeVisible();
  await page.screenshot({
    path: "../.context/keychain/workspace-empty.png",
    fullPage: true,
  });
  // Free plan: secrets can be added before any payment.
  await expect(page.getByText("Free includes 5 secrets")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "+ Add secret" }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Subscribe for $10/month", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Test Stripe Checkout" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Pay $10", exact: true }).click();
  await page
    .getByRole("button", { name: "Use an existing passkey", exact: true })
    .click();
  await expect(page.getByText("Access paid through")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "+ Add secret" }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "+ Add secret" }).click();
  await page.getByRole("button", { name: /^Store a secret/ }).click();
  await page.getByLabel("Name", { exact: true }).fill("BROWSER_SECRET");
  await page
    .getByLabel("Secret value", { exact: true })
    .fill("browser-private-value-8347");
  await page.getByRole("button", { name: "Encrypt & save" }).click();
  await expect(
    page.getByRole("heading", { name: "BROWSER_SECRET", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("No agents have access.", { exact: true }),
  ).toBeVisible();
  await page.screenshot({
    path: "../.context/keychain/workspace-secret.png",
    fullPage: true,
  });
  const identity = Keychain.generateKey();
  await page.getByLabel("Agent name", { exact: true }).fill("Browser agent");
  await page
    .getByLabel("Agent public key", { exact: true })
    .fill(identity.publicKey);
  await page
    .getByRole("button", { name: "Approve agent", exact: true })
    .click();
  await expect(page.getByText("Browser agent", { exact: true })).toBeVisible();
  for (const width of [320, 390, 768, 1024, 1440, 2560]) {
    await page.setViewportSize({ width, height: 900 });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
      `populated workspace fits at ${width}px`,
    ).toBe(true);
    const actions = page.locator(".agent-row .row-actions");
    await actions.scrollIntoViewIfNeeded();
    await expect(
      actions.getByRole("button", { name: "Revoke", exact: true }),
    ).toBeInViewport();
  }
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.getByRole("button", { name: "Revoke", exact: true }).click();
  await expect(
    page.getByText("No agents have access.", { exact: true }),
  ).toBeVisible();
  expect(
    requests.every(
      (body) =>
        !body.includes("browser-private-value-8347") &&
        !body.includes(identity.privateKey),
    ),
  ).toBeTruthy();
  // Connected service: catalog picker, manifest-driven wizard, credential check.
  await page.getByRole("button", { name: "+ Add secret" }).click();
  await expect(page.getByText("The agent never sees the key.")).toBeVisible();
  await page.screenshot({ path: "../.context/keychain/add-choose.png" });
  await page.getByRole("button", { name: /^Connect a service/ }).click();
  await expect(
    page.getByRole("button", { name: /Read Stripe balance/ }),
  ).toBeVisible();
  await page.screenshot({ path: "../.context/keychain/add-catalog.png" });
  await page.getByRole("button", { name: /Read Stripe balance/ }).click();
  await expect(
    page.getByRole("heading", { name: "Read Stripe balance", exact: true }),
  ).toBeVisible();
  await expect(page.getByText("What the agent never gets")).toBeVisible();
  await expect(
    page.getByText('await keychain.use("STRIPE_API_KEY")'),
  ).toBeVisible();
  await page.screenshot({
    path: "../.context/keychain/add-wizard.png",
    fullPage: true,
  });
  await page.getByLabel("Name", { exact: true }).fill("STRIPE_API_KEY");
  await page.getByLabel("Credential", { exact: true }).fill("not-a-stripe-key");
  await page.getByRole("button", { name: "Encrypt & connect" }).click();
  await expect(
    page.getByText(/does not look like a credential for Read Stripe balance/),
  ).toBeVisible();
  // A filled textarea contributes its value to the label's accessible name.
  // The fixture is assembled at runtime so secret scanners do not flag it.
  const stripeFixture = ["rk", "test", "browserfixture0000000000"].join("_");
  await page.getByRole("textbox", { name: /^Credential/ }).fill(stripeFixture);
  await page.getByRole("button", { name: "Encrypt & connect" }).click();
  await expect(
    page.getByRole("heading", { name: "STRIPE_API_KEY", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText(/CONNECTED SERVICE · READ STRIPE BALANCE/),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Export secret" })).toHaveCount(
    0,
  );
  await page.getByText("How agents use this service").click();
  await expect(page.getByText("In the MCP server it is the")).toBeVisible();
  await expect(
    page.getByText("STORED SECRETS", { exact: false }),
  ).toBeVisible();
  await expect(
    page.getByText("CONNECTED SERVICES", { exact: false }),
  ).toBeVisible();
  await page.screenshot({
    path: "../.context/keychain/service-detail.png",
    fullPage: true,
  });
  expect(requests.every((body) => !body.includes(stripeFixture))).toBeTruthy();
  await page.getByRole("button", { name: "Sign out", exact: true }).click();
  await page
    .getByRole("button", { name: "Use an existing passkey", exact: true })
    .click();
  await expect(
    page.getByRole("button").filter({ hasText: "BROWSER_SECRET" }),
  ).toBeVisible();
  expect(errors).toEqual([]);
  await page.screenshot({
    path: "../.context/keychain/browser-passkey.png",
    fullPage: true,
  });
});
test("Google-only sign-in verifies a nonce-bound JWT without a wallet or passkey", async ({
  page,
}) => {
  const subject = "browser-" + hex(randomBytes()).slice(0, 20);
  await page.route("https://accounts.google.com/gsi/client", async (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: `
    window.google={accounts:{id:{initialize(config){this.config=config;},renderButton(element){
      const button=document.createElement('button');button.textContent='Continue with Google';
      button.onclick=async()=>{const response=await fetch(${JSON.stringify(mockLit + "/test/google-token?sub=" + subject + "&nonce=")}+encodeURIComponent(this.config.nonce));const data=await response.json();this.config.callback({credential:data.token});};
      element.replaceChildren(button);
    }}}};`,
    }),
  );
  await page.goto("/");
  await page
    .getByRole("button", { name: "Continue with Google", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Secrets", exact: true }),
  ).toBeVisible();
  const checkout = await page.request.post("/api/billing/checkout");
  const { url } = await checkout.json();
  expect(new URL(url).origin).toBe("http://127.0.0.1:55442");
  expect((await page.request.post(url + "/complete")).ok()).toBeTruthy();
  await page
    .getByRole("button", { name: "Refresh billing", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "+ Add secret" }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "+ Add secret" }).click();
  await page.getByRole("button", { name: /^Store a secret/ }).click();
  await page.getByLabel("Name", { exact: true }).fill("GOOGLE_SECRET");
  await page
    .getByLabel("Secret value", { exact: true })
    .fill("google-private-9847");
  await page.getByRole("button", { name: "Encrypt & save" }).click();
  await expect(
    page.getByRole("heading", { name: "GOOGLE_SECRET", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Sign out", exact: true }).click();
  await page
    .getByRole("button", { name: "Continue with Google", exact: true })
    .click();
  await expect(
    page.getByRole("button").filter({ hasText: "GOOGLE_SECRET" }),
  ).toBeVisible();
});
test("RainbowKit injected wallet connects and signs an EIP-712 owner proof", async ({
  page,
}) => {
  const account = privateKeyToAccount(`0x${hex(randomBytes())}`);
  await page.exposeFunction("testWalletSign", async (payload: string) =>
    account.signTypedData(JSON.parse(payload)),
  );
  await page.addInitScript(
    ({ address }) => {
      const listeners = new Map<string, Function[]>();
      let connected = false;
      const ethereum: any = {
        isMetaMask: true,
        isConnected: () => true,
        on: (name: string, fn: Function) =>
          listeners.set(name, [...(listeners.get(name) || []), fn]),
        removeListener: () => {},
        request: async ({ method, params }: any) => {
          if (method === "eth_requestAccounts") {
            connected = true;
            return [address];
          }
          if (method === "eth_accounts") return connected ? [address] : [];
          if (method === "eth_chainId") return "0x1";
          if (method === "net_version") return "1";
          if (method === "eth_signTypedData_v4")
            return (window as any).testWalletSign(params[1]);
          if (
            method === "wallet_getPermissions" ||
            method === "wallet_requestPermissions"
          )
            return [{ parentCapability: "eth_accounts" }];
          return null;
        },
      };
      Object.defineProperty(window, "ethereum", { value: ethereum });
    },
    { address: account.address },
  );
  await page.goto("/");
  await page
    .getByRole("button", { name: "Connect Wallet", exact: true })
    .click();
  const injected = page
    .getByRole("button", { name: /MetaMask|Browser Wallet|Injected|Ethereum/i })
    .first();
  await injected.click();
  await page
    .getByRole("button", { name: "Sign in with wallet", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Secrets", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".alert.success")).toBeVisible();
  // The banner and workspace share a centered shell at every breakpoint.
  for (const width of [320, 390, 700, 768, 1024, 1440, 2560, 3840]) {
    await page.setViewportSize({ width, height: 900 });
    const layout = await page.evaluate(() => {
      const workspace = document
        .querySelector(".workspace")!
        .getBoundingClientRect();
      const alert = document
        .querySelector(".alert.success")!
        .getBoundingClientRect();
      return {
        overflow: document.documentElement.scrollWidth > window.innerWidth,
        center: workspace.x + workspace.width / 2,
        width: workspace.width,
        bannerLeft: alert.x,
        workspaceLeft: workspace.x,
        bannerWidth: alert.width,
      };
    });
    expect(layout.overflow, `page overflow at ${width}px`).toBe(false);
    expect(layout.center).toBeCloseTo(width / 2, 0);
    expect(layout.width).toBeLessThanOrEqual(1440);
    expect(layout.bannerLeft).toBeCloseTo(layout.workspaceLeft, 0);
    expect(layout.bannerWidth).toBeCloseTo(layout.width, 0);
    if ([390, 1440, 2560].includes(width)) {
      await page.screenshot({
        path: `../.context/keychain/workspace-${width}.png`,
        fullPage: true,
      });
    }
    await expect(
      page.getByRole("button", { name: "+ Add secret" }),
    ).toBeInViewport();
  }
  await page.setViewportSize({ width: 1280, height: 900 });
});
