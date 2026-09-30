import { test as base, expect, type Page } from '@playwright/test';
import { DashboardPage } from '../../fixtures/dashboard';

const api = 'http://localhost:8000/core/v1';
const invalidKey = Buffer.alloc(32, 255).toString('base64');

const test = base.extend<{ dashboard: DashboardPage; browserErrors: void }>({
  dashboard: async ({ page }, use) => { await use(new DashboardPage(page)); },
  browserErrors: [async ({ page }, use) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    // Typography is irrelevant to the auth contract; don't depend on Google
    // Fonts availability. Application JS and all happy-path API calls are real.
    await page.route('https://fonts.googleapis.com/**', route => route.fulfill({ body: '' }));
    await use();
    expect(errors, 'Uncaught browser exceptions').toEqual([]);
  }, { auto: true }],
});

async function signOut(page: Page) {
  await page.locator('#account-dropdown-trigger').click();
  await page.locator('#account-signout-btn').click();
  await expect(page.locator('#page-login')).toBeVisible();
  await expect(page.locator('#dashboard-wrap')).toBeHidden();
}

test('create an account, preserve the session, sign out and sign in with the issued key', async ({ dashboard, page, request }) => {
  await dashboard.goto();
  const input = {
    email: `browser-${Date.now()}@example.com`,
    name: 'Browser signup regression',
    description: 'Disposable local CI account',
  };
  const creation = page.waitForResponse(`${api}/new_account`);
  const key = await dashboard.createApiModeAccount(input);
  const response = await creation;
  expect(response.ok()).toBe(true);
  expect(response.request().postDataJSON()).toEqual({
    email: input.email,
    account_name: input.name,
    account_description: input.description,
  });
  expect((await response.json()).api_key).toBe(key);
  expect(key.length).toBeGreaterThanOrEqual(32);

  // Independent server read proves the browser displayed a key for an account
  // actually created on the local chain, not merely a success screen.
  const exists = await request.get(`${api}/account_exists`, { headers: { 'X-Api-Key': key } });
  expect(exists.ok()).toBe(true);
  expect(await exists.json()).toBe(true);
  await expect(page.locator('#new-account-banner')).toBeVisible();
  await page.locator('#new-account-dismiss-btn').click();
  await expect(page.locator('#new-account-banner')).toBeHidden();

  await page.reload();
  await dashboard.expectLoggedIn();
  await signOut(page);
  await page.reload();
  await dashboard.expectLoggedOut();
  await expect(page.locator('#dashboard-wrap')).toBeHidden();

  const login = page.waitForResponse(`${api}/account_exists`);
  await dashboard.loginWithApiKey(key);
  const loginResponse = await login;
  expect(loginResponse.request().headers()['x-api-key']).toBe(key);
  expect(await loginResponse.json()).toBe(true);
  await page.reload();
  await dashboard.expectLoggedIn();
  await expect(page.locator('#login-api-key')).toHaveValue('');
});

test('an unknown API key cannot enter the dashboard or survive reload as a session', async ({ dashboard, page }) => {
  await dashboard.goto();
  await page.locator('#login-api-key').fill(invalidKey);
  const response = page.waitForResponse(`${api}/account_exists`);
  await page.locator('#btn-login').click();
  expect(await (await response).json()).toBe(false);
  await expect(page.locator('#login-status')).toContainText('Account not found');
  await expect(page.locator('#btn-login')).toBeEnabled();
  await expect(page.locator('#dashboard-wrap')).toBeHidden();
  await page.reload();
  await dashboard.expectLoggedOut();
  await expect(page.locator('#dashboard-wrap')).toBeHidden();
});

test('missing signup fields never send an account-creation request', async ({ dashboard, page }) => {
  let creationRequests = 0;
  page.on('request', request => {
    if (request.url() === `${api}/new_account`) creationRequests++;
  });
  await dashboard.goto();
  await dashboard.showNewUserTab();
  await page.locator('#new-account-name').fill('Missing email');
  await page.locator('#btn-create-account').click();
  // Main uses an inline error; #719 uses native required-field validation.
  await expect(page.locator('#new-account-email')).toHaveValue('');
  await dashboard.expectLoggedOut();
  await expect(page.locator('#btn-create-account')).toBeEnabled();
  await page.locator('#new-account-email').fill('browser@example.com');
  await page.locator('#new-account-name').fill('');
  await page.locator('#btn-create-account').click();
  await expect(page.locator('#dashboard-wrap')).toBeHidden();
  expect(creationRequests).toBe(0);
});

test('a failed signup shows an error and can be retried against the real backend', async ({ dashboard, page }) => {
  await page.route(`${api}/new_account`, route => route.fulfill({
    status: 503, contentType: 'application/json', body: JSON.stringify({ error: 'Signup temporarily unavailable' }),
  }));
  await dashboard.goto();
  await dashboard.showNewUserTab();
  await page.locator('#new-account-email').fill('retry@example.com');
  await page.locator('#new-account-name').fill('Retry signup');
  await page.locator('#btn-create-account').click();
  await expect(page.locator('#login-status')).toContainText(/Error:/);
  await expect(page.locator('#btn-create-account')).toBeEnabled();
  await expect(page.locator('#dashboard-wrap')).toBeHidden();
  await expect(page.locator('#new-account-banner')).toBeHidden();
  await page.unroute(`${api}/new_account`);
  await page.locator('#btn-create-account').click();
  await dashboard.expectLoggedIn();
  await expect(page.locator('#new-account-key-text')).not.toBeEmpty();
});

test('a failed login shows an error and allows another attempt', async ({ dashboard, page }) => {
  await page.route(`${api}/account_exists`, route => route.fulfill({
    status: 503, contentType: 'application/json', body: JSON.stringify({ error: 'Login temporarily unavailable' }),
  }));
  await dashboard.goto();
  await page.locator('#login-api-key').fill(invalidKey);
  await page.locator('#btn-login').click();
  await expect(page.locator('#login-status')).toContainText(/Error:/);
  await expect(page.locator('#btn-login')).toBeEnabled();
  await expect(page.locator('#dashboard-wrap')).toBeHidden();
  await page.unroute(`${api}/account_exists`);
  await page.locator('#btn-login').click();
  await expect(page.locator('#login-status')).toContainText('Account not found');
  await expect(page.locator('#btn-login')).toBeEnabled();
});

test('account-access modes are keyboard accessible and persist on reload', async ({ dashboard, page }) => {
  await dashboard.goto();
  await page.locator('#login-auth-mode-api').press('ArrowRight');
  await expect(page.locator('#login-auth-mode-chainsecured')).toBeChecked();
  await expect(page.locator('#btn-login-wallet')).toBeVisible();
  await expect(page.locator('#login-api-key')).toBeHidden();
  await dashboard.showNewUserTab();
  await expect(page.locator('#new-chainsecured-name')).toBeVisible();
  await expect(page.locator('#new-account-email')).toBeHidden();
  await page.reload();
  await expect(page.locator('#login-auth-mode-chainsecured')).toBeChecked();
  await dashboard.showNewUserTab();
  await page.locator('#login-auth-mode-chainsecured').press('ArrowLeft');
  await expect(page.locator('#new-account-email')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(page.viewportSize()!.width);
});
