import type { Page, Locator } from '@playwright/test';
import { expect } from '@playwright/test';

/**
 * Page object for the Chipotle dashboard at
 * `lit-static/dapps/dashboard/index.html`.
 *
 * Targets stable element IDs that already exist in the dashboard markup — we
 * deliberately don't sprinkle test-only `data-testid`s into a production page.
 */
export class DashboardPage {
  constructor(public readonly page: Page) {}

  // ───── Navigation ─────────────────────────────────────────────────────────

  async goto(): Promise<void> {
    await this.page.goto('');
    await this.page.waitForLoadState('domcontentloaded');
  }

  loginSection(): Locator {
    return this.page.locator('#page-login');
  }

  dashboardWrap(): Locator {
    return this.page.locator('#dashboard-wrap');
  }

  async expectLoggedIn(): Promise<void> {
    await expect(this.dashboardWrap()).toBeVisible();
  }

  async expectLoggedOut(): Promise<void> {
    await expect(this.loginSection()).toBeVisible();
  }

  // ───── API-mode auth ──────────────────────────────────────────────────────

  /** Switch the login card to the "Existing User" tab. */
  async showExistingUserTab(): Promise<void> {
    await this.page.locator('#login-tab-existing').click();
  }

  /** Switch the login card to the "New User" tab. */
  async showNewUserTab(): Promise<void> {
    await this.page.locator('#login-tab-new').click();
  }

  /** Log in with an existing API key (API mode). */
  async loginWithApiKey(apiKey: string): Promise<void> {
    await this.showExistingUserTab();
    await this.page.locator('#login-api-key').fill(apiKey);
    await this.page.locator('#btn-login').click();
    await this.expectLoggedIn();
  }

  /**
   * Create a new managed (API mode) account through the New User tab. Returns
   * the API key shown in the post-creation banner — it is shown once and the
   * tests need to capture it for follow-up requests.
   */
  async createApiModeAccount(input: {
    email: string;
    name: string;
    description?: string;
  }): Promise<string> {
    await this.showNewUserTab();
    await this.page.locator('#new-account-email').fill(input.email);
    await this.page.locator('#new-account-name').fill(input.name);
    if (input.description !== undefined) {
      await this.page.locator('#new-account-desc').fill(input.description);
    }
    await this.page.locator('#btn-create-account').click();
    await this.expectLoggedIn();
    const banner = this.page.locator('#new-account-banner');
    await expect(banner).toBeVisible();
    const apiKey = (await this.page.locator('#new-account-key-text').textContent())?.trim();
    if (!apiKey) throw new Error('Account banner shown but API key was empty');
    return apiKey;
  }

  // ───── ChainSecured (wallet) auth ─────────────────────────────────────────

  /**
   * Click the "Connect wallet" button on the Existing User tab and pick the
   * connector option in the dashboard's wallet picker.
   */
  async startWalletLogin(connector: 'metamask' | 'walletconnect'): Promise<void> {
    await this.showExistingUserTab();
    await this.page.locator('#login-auth-mode-chainsecured').click();
    await this.page.locator('#btn-login-wallet').click();
    await this.pickWalletConnector(connector);
  }

  /**
   * Click the "Connect wallet & create" button on the New User tab and pick
   * the connector option.
   */
  async startWalletCreate(
    connector: 'metamask' | 'walletconnect',
    input: { name: string; description?: string },
  ): Promise<void> {
    await this.showNewUserTab();
    await this.page.locator('#login-auth-mode-chainsecured').click();
    await this.page.locator('#new-chainsecured-name').fill(input.name);
    if (input.description !== undefined) {
      await this.page.locator('#new-chainsecured-desc').fill(input.description);
    }
    await this.page.locator('#btn-create-chainsecured').click();
    await this.pickWalletConnector(connector);
  }

  /** Click a button in the `<dialog id="lit-wallet-picker">` modal. */
  private async pickWalletConnector(
    connector: 'metamask' | 'walletconnect',
  ): Promise<void> {
    const picker = this.page.locator('#lit-wallet-picker');
    await expect(picker).toBeVisible();
    await picker.locator(`button[data-wallet="${connector}"]`).click();
  }

  /**
   * Wait for the WalletConnect pairing URI emitted by `wallet_connect.js` as a
   * `lit:wc-display-uri` window event. The dashboard fires that event before
   * it pops the WC QR modal — tests pair their headless wallet with the URI
   * instead of scraping the QR code.
   */
  async waitForWcPairingUri(timeoutMs = 30_000): Promise<string> {
    return this.page.evaluate(
      (timeout) =>
        new Promise<string>((resolve, reject) => {
          const t = setTimeout(() => reject(new Error('Timed out waiting for WC pairing URI')), timeout);
          window.addEventListener(
            'lit:wc-display-uri',
            (ev) => {
              clearTimeout(t);
              resolve((ev as CustomEvent<string>).detail);
            },
            { once: true },
          );
        }),
      timeoutMs,
    );
  }

  // ───── Action runner ──────────────────────────────────────────────────────

  /**
   * Show the Action Runner section. It is hidden by default until a usage key
   * has been provisioned, so callers should also set the usage-key override
   * (via `setUsageKeyOverride`) or supply one inline.
   */
  async showActionRunner(): Promise<void> {
    const section = this.page.locator('#section-action-runner');
    // Force-show the section: the dashboard reveals it conditionally, but the
    // markup is always present.
    await section.evaluate((el) => {
      (el as HTMLElement).style.display = 'block';
    });
    await section.scrollIntoViewIfNeeded();
  }

  /**
   * Set the contents of a CodeJar editor (`#action-runner-code` or
   * `#action-runner-params`). CodeJar uses a contenteditable div, so we set
   * textContent and dispatch an `input` event to retrigger highlighting.
   */
  private async fillCodeJar(id: string, code: string): Promise<void> {
    const el = this.page.locator(`#${id}`);
    await el.waitFor({ state: 'attached' });
    await el.evaluate((node, value) => {
      node.textContent = value;
      node.dispatchEvent(new Event('input', { bubbles: true }));
    }, code);
  }

  /**
   * Run a Lit Action through the Action Runner UI and return the parsed JSON
   * the output panel displays. The panel always renders the server's response
   * verbatim as JSON, so we round-trip through JSON.parse.
   */
  async runLitAction(input: {
    usageApiKey: string;
    code: string;
    jsParams?: unknown;
  }): Promise<{
    raw: string;
    parsed: unknown;
    has_error?: boolean;
    response?: unknown;
  }> {
    await this.showActionRunner();
    await this.page.locator('#action-runner-usage-key').fill(input.usageApiKey);
    await this.fillCodeJar('action-runner-code', input.code);
    await this.fillCodeJar(
      'action-runner-params',
      input.jsParams === undefined ? '' : JSON.stringify(input.jsParams, null, 2),
    );

    const output = this.page.locator('#action-runner-output');
    await this.page.locator('#btn-execute-lit-action').click();
    // The runner writes "Executing…" then replaces it with JSON on success or
    // a message starting with "Error: " on failure. Action execution can
    // easily exceed the global expect timeout (15s) — pass our own.
    await expect(output).not.toHaveText(/^Executing/, { timeout: 60_000 });
    const raw = (await output.textContent())?.trim() ?? '';
    if (raw.startsWith('Error:')) {
      throw new Error(`Action runner failed: ${raw}`);
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch (err) {
      throw new Error(
        `Action runner output was not JSON: ${(err as Error).message}\n---\n${raw}`,
      );
    }
    const obj = (parsed ?? {}) as { has_error?: boolean; response?: unknown };
    return { raw, parsed, has_error: obj.has_error, response: obj.response };
  }

  /** Resolve a snippet's IPFS CID via the runner's "Get Lit Action IPFS CID" button. */
  async getLitActionIpfsCid(code: string): Promise<string> {
    await this.showActionRunner();
    await this.fillCodeJar('action-runner-code', code);
    const output = this.page.locator('#action-runner-output');
    await this.page.locator('#btn-get-lit-action-ipfs-cid').click();
    // CID hashing/IPFS round-trip can exceed the 15s global expect timeout.
    await expect(output).not.toHaveText(/^Fetching/, { timeout: 30_000 });
    const raw = (await output.textContent())?.trim() ?? '';
    if (raw.startsWith('Error:')) {
      throw new Error(`Get-CID failed: ${raw}`);
    }
    return raw;
  }

  // ───── Usage-key override ─────────────────────────────────────────────────

  /**
   * Apply a usage-key override so the dashboard makes its writes with the
   * provided usage key instead of the account API key. Opens the panel via
   * the account dropdown if it isn't already visible.
   */
  async setUsageKeyOverride(usageApiKey: string): Promise<void> {
    const card = this.page.locator('#usage-key-override-card');
    if (!(await card.isVisible())) {
      await this.page.locator('#account-dropdown-trigger').click();
      await this.page.locator('#toggle-usage-override-btn').click();
    }
    await this.page.locator('#usage-key-override-input').fill(usageApiKey);
    await this.page.locator('#usage-key-override-apply').click();
    await expect(this.page.locator('#usage-key-override-badge')).toBeVisible();
  }

  // ───── Misc verifications ─────────────────────────────────────────────────

  /** Read the wallet address rendered as the PKP in the wallets table, if any. */
  async firstWalletAddress(): Promise<string | null> {
    const rows = this.page.locator('#wallets-tbody tr');
    if ((await rows.count()) === 0) return null;
    const addr = (await rows.first().locator('td').nth(1).textContent())?.trim();
    return addr || null;
  }
}
