import assert from 'node:assert/strict';
import { test, beforeEach } from 'node:test';
import { getBillingSessionHeader, forgetBillingSession, hasSignedBillingSession } from './billing_session.js';
import { buildBillingSessionTypedData } from '../../core_sdk.js';
import { disconnectWallet, logOut } from './auth.js';
import { onWalletChange } from '../../wallet_connect.js';

const wallet = '0x00000000000000000000000000000000000000BB';
const keyAddress = '0x00000000000000000000000000000000000000AA';
const storageKey = `lit:billing-session:v1:8453:${wallet.toLowerCase()}`;
function storage() {
  return Object.defineProperties({}, {
    getItem: { value(key) { return this[key] ?? null; } },
    setItem: { value(key, value) { this[key] = String(value); } },
    removeItem: { value(key) { delete this[key]; } },
  });
}
function deferred() {
  let resolve;
  const promise = new Promise((r) => { resolve = r; });
  return { promise, resolve };
}
const tick = () => new Promise((resolve) => setImmediate(resolve));
class Wallet {
  address = keyAddress;
  privateKey = 'mock-session-key';
  static createRandom() { return new Wallet(); }
  async signMessage() { return 'mock-proof'; }
}
function storedSession(signature = null) {
  return {
    sessionPrivateKey: 'mock-session-key',
    typedData: buildBillingSessionTypedData({ address: wallet, sessionKey: keyAddress, chainId: 8453, issuedAt: Math.floor(Date.now() / 1000) }),
    signature,
    safe: { threshold: 2, messageHash: 'mock-hash' },
    createdAt: Math.floor(Date.now() / 1000),
  };
}
beforeEach(() => {
  globalThis.localStorage = storage();
  globalThis.sessionStorage = storage();
  globalThis.ethers = { Wallet };
  globalThis.document = {
    getElementById: () => null,
    querySelector: () => null,
    body: { classList: { toggle() {} } },
  };
});

test('sign-out cancels shared wallet prompts and late signatures cannot overwrite a new session', async () => {
  const started = deferred();
  const signature = deferred();
  let prompts = 0;
  const opts = { wallet, chainId: 8453, connect: async () => ({ signer: {
    signTypedData() { prompts++; started.resolve(); return signature.promise; },
  } }) };
  const first = getBillingSessionHeader(opts);
  const shared = getBillingSessionHeader(opts);
  const cancelled = [first, shared].map((p) => assert.rejects(p, { name: 'AbortError' }));
  await started.promise;
  forgetBillingSession(wallet);
  await Promise.all(cancelled);
  assert.equal(prompts, 1);
  assert.equal(localStorage.getItem(storageKey), null);
  await getBillingSessionHeader({ ...opts, connect: async () => ({ signer: { signTypedData: async () => 'new-signature' } }) });
  signature.resolve('old-signature');
  await tick();
  assert.equal(JSON.parse(localStorage.getItem(storageKey)).signature, 'new-signature');
});

test('sign-out cancels resumed Safe polling and ignores a late service response', async () => {
  localStorage.setItem(storageKey, JSON.stringify(storedSession()));
  const polling = deferred();
  const response = deferred();
  let calls = 0;
  globalThis.fetch = async () => {
    calls++;
    if (calls === 1) return { ok: true, json: async () => ({ confirmations: [{}] }) };
    polling.resolve();
    return response.promise;
  };
  const header = getBillingSessionHeader({ wallet, chainId: 8453, connect: () => assert.fail('must resume without prompting') });
  const cancelled = assert.rejects(header, { name: 'AbortError' });
  await polling.promise;
  forgetBillingSession(wallet);
  await cancelled;
  response.resolve({ ok: true, json: async () => ({ confirmations: [{}, {}], preparedSignature: 'late-safe-signature' }) });
  await tick();
  assert.equal(localStorage.getItem(storageKey), null);
  assert.equal(calls, 2);
});

test('explicit disconnect clears sessions before the synchronous wallet callback', async () => {
  localStorage.setItem(storageKey, JSON.stringify(storedSession('signed')));
  sessionStorage.setItem('accountconfig_mode', 'sovereign');
  sessionStorage.setItem('accountconfig_chainsecured_wallet', wallet);
  sessionStorage.setItem('accountconfig_chainsecured_hash', 'hash');
  let callbackRan = false;
  const unsubscribe = onWalletChange(() => {
    callbackRan = true;
    logOut();
  });
  try {
    await disconnectWallet();
    await tick();
    assert.equal(callbackRan, true);
    assert.equal(hasSignedBillingSession(wallet), false);
    assert.equal(localStorage.getItem(storageKey), null);
  } finally { unsubscribe(); }
});

test('automatic logout preserves a signed session; explicit logout removes it synchronously', async () => {
  localStorage.setItem(storageKey, JSON.stringify(storedSession('signed')));
  sessionStorage.setItem('accountconfig_chainsecured_wallet', wallet);
  logOut();
  assert.equal(hasSignedBillingSession(wallet), true);
  sessionStorage.setItem('accountconfig_chainsecured_wallet', wallet);
  logOut({ forgetWalletSessions: true });
  assert.equal(hasSignedBillingSession(wallet), false);
  await tick();
});

test('sign-out while connecting prevents the subsequent signing prompt', async () => {
  const connecting = deferred();
  const connection = deferred();
  const header = getBillingSessionHeader({ wallet, chainId: 8453, connect: () => {
    connecting.resolve();
    return connection.promise;
  } });
  const cancelled = assert.rejects(header, { name: 'AbortError' });
  await connecting.promise;
  forgetBillingSession(wallet);
  await cancelled;
  let prompted = false;
  connection.resolve({ signer: { signTypedData() { prompted = true; return Promise.resolve('late'); } } });
  await tick();
  assert.equal(prompted, false);
  assert.equal(localStorage.getItem(storageKey), null);
});
