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
    command: "node --import tsx passkey-server.ts",
    cwd: import.meta.dirname,
    // This endpoint exists only after the entire UI + identity build succeeds.
    url: "http://localhost:55449/identities.js",
    timeout: 120000,
    reuseExistingServer: false,
    gracefulShutdown: { signal: "SIGTERM", timeout: 5000 },
    stdout: "pipe",
    stderr: "pipe",
  },
});
