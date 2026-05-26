import { defineConfig, devices } from '@playwright/test';

/**
 * The dashboard is served by static-web-server from `lit-static/`
 * (see local_test.sh step 7). When opened from a localhost origin it talks to
 * lit-api-server at http://localhost:8000 directly — no proxy needed.
 */
const DASHBOARD_URL =
  process.env.DASHBOARD_URL ?? 'http://localhost:8080/dapps/dashboard/';
export const API_BASE_URL =
  process.env.API_BASE_URL ?? 'http://localhost:8000';

export default defineConfig({
  testDir: './tests',
  globalSetup: './fixtures/global-setup.ts',
  // Synpress + the dashboard's WalletConnect picker don't tolerate aggressive
  // parallelism. Pure-API specs can scale, but we share a single MM extension
  // cache so we hold workers low.
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 2 : 1,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',

  // Lit Action runs can take a few seconds inside the dstack simulator; give
  // tests room without letting a stuck WC pairing hang CI forever.
  timeout: 90_000,
  expect: { timeout: 15_000 },

  use: {
    baseURL: DASHBOARD_URL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },

  projects: [
    {
      name: 'api-mode',
      testMatch: /api\/.*\.spec\.ts/,
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'eoa',
      testMatch: /eoa\/.*\.spec\.ts/,
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'walletconnect',
      testMatch: /walletconnect\/.*\.spec\.ts/,
      use: { ...devices['Desktop Chrome'] },
    },
    // Flow specs are split per mode (one file each under tests/flows/) so the
    // API project doesn't pull in the WC fixture and the WC project doesn't
    // hit the API account-creation path before its prereqs are ready. The
    // EOA flow lives in tests/eoa/ — those specs need Synpress anyway.
    {
      name: 'flows-api',
      testMatch: /flows\/api-.*\.spec\.ts/,
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'flows-wc',
      testMatch: /flows\/wc-.*\.spec\.ts/,
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  // Boot Anvil, dstack-simulator, contracts, lit-api-server, lit-actions and
  // static-web-server via `./local_test.sh` before running the suite (Makefile
  // `make up` handles this). We don't auto-start here — the cargo builds are
  // too slow for Playwright's webServer health-check loop and dstack needs a
  // host-installed simulator.
});
