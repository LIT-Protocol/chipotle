import { defineConfig } from "@playwright/test";

// Browser identity regression tests: real WebAuthn, only the public lookup is mocked.
// Run: npx playwright test --config tests/passkey.playwright.config.ts
export default defineConfig({
  testDir: ".",
  testMatch: "passkey.browser.spec.ts",
  workers: 1,
  timeout: 30000,
  use: { baseURL: "http://localhost:55449", headless: true },
  webServer: {
    command: "npm run dev -- --port 55449 --strictPort",
    url: "http://localhost:55449",
    reuseExistingServer: false,
  },
});
