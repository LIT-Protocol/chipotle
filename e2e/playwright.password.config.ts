import { defineConfig, devices } from "@playwright/test";
export default defineConfig({
  testDir: "./password",
  fullyParallel: false,
  workers: 1,
  timeout: 90000,
  expect: { timeout: 15000 },
  forbidOnly: !!process.env.CI,
  use: {
    ...devices["Desktop Chrome"],
    baseURL: "http://localhost:8080/dapps/dashboard/",
    screenshot:
      process.env.AGENT_SCREENSHOTS === "1" ? "on" : "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "node ../lit-dashboard-auth/test/browser-server.mjs",
    url: "http://localhost:8080/dapps/dashboard/",
    reuseExistingServer: false,
    timeout: 30000,
  },
});
