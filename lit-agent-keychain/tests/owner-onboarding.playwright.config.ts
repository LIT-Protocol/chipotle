import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: ".",
  testMatch: "owner-onboarding.browser.spec.ts",
  workers: 1,
  use: {
    baseURL: "http://127.0.0.1:55449",
    headless: true,
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "npx vite --config owner-onboarding.vite.config.ts",
    url: "http://127.0.0.1:55449/fixtures/owner-onboarding.html",
    reuseExistingServer: false,
  },
});
