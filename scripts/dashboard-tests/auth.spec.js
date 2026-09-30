import { test, expect } from '@playwright/test';

const dashboard = '/dapps/dashboard/';

test.beforeEach(async ({ page }) => {
  // Keep tests offline: auth uses the real SDK with mocked API responses.
  await page.route(/^https:\/\//, route => route.fulfill({ body: '' }));
  await page.route('http://localhost:8000/**', route => {
    const path = new URL(route.request().url()).pathname;
    const data = path.endsWith('/account_exists') ? true
      : path.endsWith('/new_account') ? { api_key: 'test-account-key', wallet_address: '0x123' }
      : path.includes('/billing/') ? {}
      : [];
    return route.fulfill({ json: data });
  });
});

test('direct entry, refresh, native links and browser history select the correct page', async ({ page }) => {
  await page.goto(`${dashboard}#create-account`);
  await expect(page.getByRole('heading', { name: 'Create an account' })).toBeVisible();
  await expect(page.getByLabel('Email', { exact: true })).toBeVisible();
  await expect(page.locator('#login-api-key')).toBeHidden();
  await page.reload();
  await expect(page).toHaveTitle('Create an account · Chipotle Dashboard');
  await page.locator('#login-sign-in-link a').click();
  await expect(page).toHaveURL(/#sign-in$/);
  await expect(page.getByRole('heading', { name: 'Sign in' })).toBeFocused();
  await page.goBack();
  await expect(page.getByLabel('Email', { exact: true })).toBeVisible();
  await page.goForward();
  await expect(page.locator('#login-api-key')).toBeVisible();
});

test('legacy dashboard URL defaults to sign-in and serves the Lit mark', async ({ page }) => {
  await page.goto(`${dashboard}index.html`);
  await expect(page).toHaveURL(/#sign-in$/);
  await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible();
  await expect(page.locator('.login-brand img')).toHaveJSProperty('naturalWidth', 311);
});

test('wallet selection supports the keyboard and persists across routes and refresh', async ({ page }) => {
  await page.goto(`${dashboard}#sign-in`);
  await page.getByRole('radio', { name: 'API key', exact: true }).press('ArrowRight');
  await expect(page.getByRole('radio', { name: 'Wallet · ChainSecured' })).toBeChecked();
  await expect(page.getByRole('button', { name: 'Connect wallet', exact: true })).toBeVisible();
  await expect(page.locator('#login-api-key')).toBeHidden();
  await page.locator('#login-create-link a').click();
  await expect(page.locator('#new-chainsecured-name')).toBeVisible();
  await expect(page.locator('#new-account-email')).toBeHidden();
  await page.reload();
  await expect(page.locator('#new-chainsecured-name')).toBeVisible();
  await page.getByRole('radio', { name: 'Wallet · ChainSecured' }).press('ArrowLeft');
  await expect(page.locator('#new-account-email')).toBeVisible();
});

test('failed sign-in stays on its route and clears errors when navigating', async ({ page }) => {
  await page.route('**/account_exists', route => route.fulfill({ json: false }));
  await page.goto(`${dashboard}#sign-in`);
  await page.locator('#login-api-key').fill('invalid-test-key');
  await page.locator('#login-api-key').press('Enter');
  await expect(page.locator('#login-status')).toContainText('Account not found');
  await expect(page).toHaveURL(/#sign-in$/);
  await page.locator('#login-create-link a').click();
  await expect(page.locator('#login-status')).toBeHidden();
});

test('Enter signs in, refresh restores the session, and sign-out returns to sign-in', async ({ page }) => {
  await page.goto(`${dashboard}#sign-in`);
  await page.locator('#login-api-key').fill('test-account-key');
  await page.locator('#login-api-key').press('Enter');
  await expect(page).toHaveURL(/#overview$/);
  await expect(page.locator('#dashboard-wrap')).toBeVisible();
  await expect(page.locator('#login-api-key')).toHaveValue('');
  await page.reload();
  await expect(page.locator('#dashboard-wrap')).toBeVisible();
  await page.locator('#account-dropdown-trigger').click();
  await page.getByRole('menuitem', { name: 'Sign out', exact: true }).click();
  await expect(page).toHaveURL(/#sign-in$/);
  await expect(page.locator('#dashboard-wrap')).toBeHidden();
});

test('creation validates email and preserves the new API key banner', async ({ page }) => {
  await page.goto(`${dashboard}#create-account`);
  await page.locator('#new-account-email').fill('invalid');
  await page.locator('#new-account-name').fill('Test account');
  await page.getByRole('button', { name: 'Create account', exact: true }).click();
  await expect(page).toHaveURL(/#create-account$/);
  await expect(page.locator('#new-account-email')).toBeFocused();
  await page.locator('#new-account-email').fill('test@example.com');
  const request = page.waitForRequest('**/new_account');
  await page.locator('#new-account-name').press('Enter');
  expect((await request).postDataJSON()).toEqual({
    account_name: 'Test account', account_description: '', email: 'test@example.com',
  });
  await expect(page).toHaveURL(/#overview$/);
  await expect(page.locator('#new-account-banner')).toBeVisible();
  await expect(page.locator('#new-account-key-text')).toHaveText('test-account-key');
});

for (const theme of ['light', 'dark']) {
  test(`mobile forms fit the viewport in ${theme} mode`, async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 });
    await page.addInitScript(theme => sessionStorage.setItem('accountconfig_theme', theme), theme);
    await page.goto(`${dashboard}#create-account`);
    await expect(page.getByRole('heading', { name: 'Create an account' })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(375);
    await page.getByRole('radio', { name: 'Wallet · ChainSecured' }).click();
    await expect(page.locator('#btn-create-chainsecured')).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(375);
  });
}
