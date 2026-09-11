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
  await page
    .getByRole("button", { name: "Create a passkey", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Secrets", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "+ Add secret" }).click();
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
  const identity = Keychain.generateKey();
  await page.getByLabel("Agent name", { exact: true }).fill("Browser agent");
  await page
    .getByLabel("Agent public key", { exact: true })
    .fill(identity.publicKey);
  await page
    .getByRole("button", { name: "Approve agent", exact: true })
    .click();
  await expect(page.getByText("Browser agent", { exact: true })).toBeVisible();
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
  await page.getByRole("button", { name: "Sign out", exact: true }).click();
  await page
    .getByRole("button", { name: "Use an existing passkey", exact: true })
    .click();
  await expect(
    page.getByRole("button").filter({ hasText: "BROWSER_SECRET" }),
  ).toBeVisible();
  expect(errors).toEqual([]);
  await page.screenshot({
    path: "../.context/keychain-v2/browser-passkey.png",
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
  await page.getByRole("button", { name: "+ Add secret" }).click();
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
});
