import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

// Run the browser module with controlled auth, DOM and network boundaries.
const source = readFileSync(new URL('./auto_recharge.js', import.meta.url), 'utf8')
  .replace(/^import .*;\n/gm, '')
  .replace(/^export /gm, '');
const flush = () => new Promise((resolve) => setImmediate(resolve));
function deferred() {
  let resolve;
  const promise = new Promise((r) => { resolve = r; });
  return { promise, resolve };
}
function element() {
  return {
    children: [], textContent: '',
    set innerHTML(_) { this.children = []; },
    appendChild(child) { this.children.push(child); },
    addEventListener() {},
  };
}
function text(element) {
  return [element.textContent, ...element.children.map(text)].join(' ');
}
const enabled = {
  enabled: true, threshold_cents: 500, topup_amount_cents: 1000,
  monthly_cap_cents: null, payment_method_id: 'pm_A',
  card_brand: 'visa', card_last4: '4242',
};
function harness(fetch) {
  let account = 'A';
  const banner = element();
  const context = vm.createContext({
    console, setTimeout, fetch,
    getMode: () => 'api', getApiKey: () => account,
    isAuthenticated: () => !!account,
    getLitPaymentsBaseUrl: () => '', onWalletAuthRejected() {},
    document: { createElement: element, getElementById: (id) => id === 'auto-recharge-banner' ? banner : null },
  });
  vm.runInContext(source, context);
  return {
    login: (value) => { account = value; },
    save: () => vm.runInContext('handleSave()', context),
    refresh: () => vm.runInContext('refreshAutoRechargeStatus()', context),
    banner: () => text(banner),
  };
}
function response(config) {
  return { status: 200, ok: true, json: async () => config, text: async () => JSON.stringify(config) };
}

test('delayed post-save card lookup cannot overwrite a newly signed-in account', async () => {
  const old = deferred();
  let reads = 0;
  const ui = harness(async (_, init) => init.method === 'PUT'
    ? response(enabled) : ++reads === 1 ? old.promise : response({ enabled: false }));
  await ui.save();
  await flush();
  ui.login('B');
  await ui.refresh();
  assert.match(ui.banner(), /Auto recharge is off/);
  old.resolve(response(enabled));
  await flush();
  assert.match(ui.banner(), /Auto recharge is off/);
  assert.doesNotMatch(ui.banner(), /4242/);
});

test('post-save card lookup is discarded after logout without another refresh', async () => {
  const old = deferred();
  const ui = harness(async (_, init) => init.method === 'PUT'
    ? response({ ...enabled, card_brand: null, card_last4: null }) : old.promise);
  await ui.save();
  await flush();
  ui.login(null);
  old.resolve(response(enabled));
  await flush();
  assert.doesNotMatch(ui.banner(), /4242/);
});

test('an older post-save lookup cannot undo a later disable', async () => {
  const old = deferred();
  let writes = 0, reads = 0;
  const ui = harness(async (_, init) => {
    if (init.method === 'PUT') return response(++writes === 1 ? enabled : { enabled: false });
    return ++reads === 1 ? old.promise : response({ enabled: false });
  });
  await ui.save();
  await flush();
  await ui.save();
  await flush();
  old.resolve(response(enabled));
  await flush();
  assert.match(ui.banner(), /Auto recharge is off/);
});

test('an earlier sign-in refresh cannot overwrite a successful save and card lookup', async () => {
  const old = deferred();
  let reads = 0;
  const ui = harness(async (_, init) => init.method === 'PUT'
    ? response(enabled) : ++reads === 1 ? old.promise : response(enabled));
  const refreshing = ui.refresh();
  await flush();
  await ui.save();
  await flush();
  assert.match(ui.banner(), /Visa •••• 4242/);
  old.resolve(response({ enabled: false }));
  await refreshing;
  assert.match(ui.banner(), /Auto recharge is on/);
  assert.match(ui.banner(), /Visa •••• 4242/);
});
