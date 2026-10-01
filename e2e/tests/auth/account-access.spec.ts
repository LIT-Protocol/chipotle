import { test as base, expect, type Page } from '@playwright/test';
import { DashboardPage } from '../../fixtures/dashboard';

const api = 'http://localhost:8000/core/v1';
const invalidKey = Buffer.alloc(32, 255).toString('base64');

const test = base.extend<{ dashboard: DashboardPage; browserErrors: void }>({
  dashboard: async ({ page }, use) => { await use(new DashboardPage(page)); },
  browserErrors: [async ({ page }, use) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    // Account access doesn't use Stripe checkout, syntax highlighting, or web
    // fonts. Keep those third-party assets out of this local regression check.
    // Application JS and all happy-path API calls are real.
    await page.route(/^https:\/\/(fonts\.googleapis\.com|fonts\.gstatic\.com|js\.stripe\.com|cdnjs\.cloudflare\.com)\//,
      route => route.fulfill({ body: '' }));
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
  await page.locator('#login-auth-mode-api').click();
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
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(page.viewportSize()!.width);
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
  await page.locator('#login-auth-mode-api').click();
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
  await page.locator('#login-auth-mode-api').click();
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
  await page.locator('#login-auth-mode-api').click();
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
  await page.locator('#login-auth-mode-api').click();
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

test('account-access modes are keyboard accessible and password is the reload default', async ({ dashboard, page }) => {
  await dashboard.goto();
  await page.locator('#login-auth-mode-api').click();
  await page.locator('#login-auth-mode-api').press('ArrowRight');
  await expect(page.locator('#login-auth-mode-chainsecured')).toBeChecked();
  await expect(page.locator('#btn-login-wallet')).toBeVisible();
  await expect(page.locator('#login-api-key')).toBeHidden();
  await dashboard.showNewUserTab();
  await expect(page.locator('#new-chainsecured-name')).toBeVisible();
  await expect(page.locator('#new-account-email')).toBeHidden();
  await page.reload();
  await expect(page.locator('#login-auth-mode-password')).toBeChecked();
  await dashboard.showNewUserTab();
  await page.locator('#login-auth-mode-password').press('ArrowRight');
  await expect(page.locator('#new-account-email')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(page.viewportSize()!.width);
});

// Actual browser crypto, Worker/D1, Rust API and Anvil account creation. Only
// email delivery is intercepted, in the local test server (never in production).
test('password-manager style signup and autofill unlock a real account', async ({ dashboard, page, request }, testInfo) => {
  const email = `password-${testInfo.project.name}-${Date.now()}@example.com`;
  const password = 'Generated-7vN!x4R@q9L#t2K$';
  const replacement = 'Generated-8hM!z6W@p3B#s5J$';
  const transmitted: string[] = [];
  page.on('request', req => {
    const body = req.postData();
    if (body) transmitted.push(body);
    expect(req.url()).not.toContain(encodeURIComponent(password));
  });
  await dashboard.goto();
  await expect(page.locator('#login-auth-mode-password')).toBeChecked();
  await dashboard.showNewUserTab();
  await page.locator('#password-signup-email').fill(email);
  await page.locator('#password-signup-form button').click();
  await expect(page.locator('#login-status')).toContainText('verification link');
  let link = '';
  await expect.poll(async () => {
    const mail = await (await request.get('http://localhost:8088/__test/mail')).json();
    link = mail.find((item: { to: string }) => item.to === email)?.text.match(/http:\/\/localhost:8088\/\S+/)?.[0] || '';
    return !!link;
  }).toBe(true);
  await page.goto(link);
  await page.locator('#password-verify-form button').click();
  const signup = page.locator('#password-create-form');
  await expect(signup).toBeVisible();
  await expect(signup).toHaveAttribute('method', 'post');
  await expect(page.locator('#password-create-email')).toHaveValue(email);
  await expect(page.locator('#password-create-email')).toHaveAttribute('autocomplete', 'username');
  await expect(page.locator('#password-create-password')).toHaveAttribute('autocomplete', 'new-password');
  await expect(page.locator('#password-create-password')).toHaveAttribute('passwordrules', /minlength: 15/);
  // Direct assignment deliberately emits no input/change events, like some
  // managers. Generated passwords must work without manually typing a character.
  await page.locator('#password-create-password').evaluate((input, value) => { (input as HTMLInputElement).value = value; }, password);
  expect(await signup.evaluate(form => Object.fromEntries(new FormData(form as HTMLFormElement)))).toMatchObject({ username: email, 'new-password': password });
  // Simulate a pre-handler rate-limit rejection, then retry against the real
  // API. The Worker must release only the rejected operation's creation claim.
  await page.route(`${api}/new_account`, route => route.fulfill({ status: 429, json: { error: 'Too many account requests' } }));
  await page.locator('#password-create-submit').click();
  await expect(page.locator('#login-status')).toContainText('rejected');
  await expect(page.locator('#dashboard-wrap')).toBeHidden();
  await page.unroute(`${api}/new_account`);
  await page.locator('#password-create-password').evaluate((input, value) => { (input as HTMLInputElement).value = value; }, password);
  const created = page.waitForResponse(`${api}/new_account`);
  await Promise.all([page.waitForNavigation(), page.locator('#password-create-submit').click()]);
  expect((await created).ok()).toBe(true);
  await dashboard.expectLoggedIn();
  const key = await page.evaluate(() => sessionStorage.getItem('accountconfig_api_key'));
  expect(key).toBeTruthy();
  const exists = await request.get(`${api}/account_exists`, { headers: { 'X-Api-Key': key! } });
  expect(exists.ok()).toBe(true);
  expect(await exists.json()).toBe(true);
  const records = await (await request.get('http://localhost:8088/__test/records')).json();
  const stored = records.find((record: { email: string }) => record.email === email);
  expect(stored.state).toBe('active');
  expect(JSON.stringify(stored)).not.toContain(key!);
  expect(JSON.stringify(stored)).not.toContain(password);
  await page.reload();
  await dashboard.expectLoggedIn();
  await signOut(page);
  await page.locator('#password-login-email').fill(email);
  await page.locator('#password-login-password').fill('This password is incorrect!');
  await page.locator('#password-login-form button[type=submit]').click();
  await expect(page.locator('#login-status')).toContainText('incorrect');
  await expect(page.locator('#dashboard-wrap')).toBeHidden();
  const fillLogin = async (value: string) => {
    await page.evaluate(({ email, value }) => {
      (document.getElementById('password-login-email') as HTMLInputElement).value = email;
      (document.getElementById('password-login-password') as HTMLInputElement).value = value;
    }, { email, value });
  };
  await fillLogin(password);
  await expect(page.locator('#password-login-password')).toHaveAttribute('autocomplete', 'current-password');
  await Promise.all([page.waitForNavigation(), page.locator('#password-login-password').press('Enter')]);
  await dashboard.expectLoggedIn();
  expect(await page.evaluate(() => sessionStorage.getItem('accountconfig_api_key'))).toBe(key);
  await page.locator('#account-dropdown-trigger').click();
  await page.locator('#password-settings-open').click();
  await expect(page.locator('#password-change-username')).toHaveValue(email);
  await page.locator('#password-current').fill(password);
  await page.locator('#password-new').evaluate((input, value) => { (input as HTMLInputElement).value = value; }, replacement);
  await Promise.all([page.waitForNavigation(), page.locator('#password-change-form button').click()]);
  await expect(page.locator('#login-status')).toContainText('Password changed');
  await fillLogin(password);
  await page.locator('#password-login-form button[type=submit]').click();
  await expect(page.locator('#login-status')).toContainText('incorrect');
  await fillLogin(replacement);
  await Promise.all([page.waitForNavigation(), page.locator('#password-login-form button[type=submit]').click()]);
  await dashboard.expectLoggedIn();
  expect(await page.evaluate(() => sessionStorage.getItem('accountconfig_api_key'))).toBe(key);
  expect(await page.evaluate(() => JSON.stringify({ ...sessionStorage }))).not.toContain(replacement);
  expect(transmitted.join('\n')).not.toContain(password);
  expect(transmitted.join('\n')).not.toContain(replacement);
});
