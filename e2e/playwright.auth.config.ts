import { defineConfig, devices } from '@playwright/test';

// Separate from the wallet/Action suite: boot only the services account access
// needs. No global wallet fixtures, production endpoints, or CI secrets.
export default defineConfig({
  testDir: './tests/auth',
  forbidOnly: !!process.env.CI,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 60_000,
  expect: { timeout: 15_000 },
  outputDir: 'test-results/auth',
  reporter: [['list'], ['html', { outputFolder: 'playwright-report/auth', open: 'never' }]],
  use: {
    baseURL: 'http://localhost:8088/dapps/dashboard/',
    trace: 'retain-on-failure',
    screenshot: process.env.AGENT_SCREENSHOTS === '1' ? 'on' : 'only-on-failure',
    video: 'retain-on-failure',
  },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'] } },
    { name: 'mobile', use: { ...devices['Pixel 7'], defaultBrowserType: 'chromium' } },
  ],
  webServer: {
    command: 'bash scripts/start-auth-stack.sh',
    url: 'http://localhost:8088/dapps/dashboard/',
    timeout: 120_000,
    reuseExistingServer: false,
    gracefulShutdown: { signal: 'SIGTERM', timeout: 10_000 },
  },
});
