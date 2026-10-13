import assert from 'node:assert/strict';
import { test } from 'node:test';
import { randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createAnalytics } from './analytics.js';

function browser(hostname, jar = { value: '' }) {
  const requests = [];
  const document = { referrer: 'https://search.example/query?token=referrer-secret' };
  Object.defineProperty(document, 'cookie', {
    get: () => jar.value,
    set: (v) => { jar.value = v.split(';')[0]; },
  });
  return {
    document, crypto: { randomUUID }, requests,
    location: { hostname, protocol: 'https:', origin: `https://${hostname}`,
      pathname: '/', href: `https://${hostname}/?utm_source=newsletter&token=super-secret#private-key` },
    fetch: async (url, options) => { requests.push({ url, options, payload: JSON.parse(options.body) }); return {}; },
  };
}

test('marketing → keychain shares a visitor/session and merges only the anonymous identity', () => {
  const jar = { value: '' };
  const marketing = browser('litprotocol.com', jar);
  createAnalytics('marketing', marketing).page('/');
  const keychain = browser('keychain.litprotocol.com', jar);
  const analytics = createAnalytics('keychain', keychain);
  analytics.page('sign-in');
  const first = marketing.requests[0].payload.properties;
  assert.equal(keychain.requests[0].payload.properties.distinct_id, first.distinct_id);
  assert.equal(keychain.requests[0].payload.properties.$session_id, first.$session_id);
  analytics.identify('keychain-owner:alice');
  const identify = keychain.requests[1].payload;
  assert.equal(identify.event, '$identify');
  assert.equal(identify.properties.$anon_distinct_id, first.distinct_id);
  analytics.capture('login_completed', { method: 'passkey' });
  assert.equal(keychain.requests.at(-1).payload.properties.distinct_id, 'keychain-owner:alice');
});

test('switching owners does not merge two people and clears old billing groups', () => {
  const b = browser('keychain.litprotocol.com');
  const analytics = createAnalytics('keychain', b);
  analytics.identify('keychain-owner:alice');
  analytics.group('keychain:' + 'a'.repeat(64));
  analytics.identify('keychain-owner:bob');
  const bob = b.requests.at(-1).payload;
  assert.equal(bob.properties.distinct_id, 'keychain-owner:bob');
  assert.notEqual(bob.properties.$anon_distinct_id, 'keychain-owner:alice');
  assert.equal(bob.properties.$groups, undefined);
  analytics.reset();
  analytics.capture('login_started');
  assert.notEqual(b.requests.at(-1).payload.properties.distinct_id, 'keychain-owner:bob');
  assert.equal(b.requests.at(-1).payload.properties.$groups, undefined);
});

test('outbound events exclude query strings, fragments, input values and error text', () => {
  const b = browser('keychain.litprotocol.com');
  const analytics = createAnalytics('keychain', b);
  analytics.capture('product_operation_failed', { operation: 'secret_create',
    secret: 'plaintext-secret', error: 'error-with-credential', password: 'password-secret' });
  const { options, payload } = b.requests[0];
  for (const secret of ['super-secret', 'private-key', 'referrer-secret', 'plaintext-secret', 'error-with-credential', 'password-secret']) {
    assert.ok(!options.body.includes(secret));
  }
  assert.equal(payload.properties.utm_source, 'newsletter');
  assert.equal(payload.properties.$referring_domain, 'search.example');
  assert.equal(options.credentials, 'omit');
  assert.equal(options.referrerPolicy, 'no-referrer');
  assert.equal(options.keepalive, true);
});

test('staging/localhost never send production events or write cookies', () => {
  for (const hostname of ['localhost', '127.0.0.1', 'next.dashboard.chipotle.litprotocol.com', 'preview.vercel.app']) {
    const b = browser(hostname);
    const analytics = createAnalytics('dashboard', b);
    analytics.page('overview');
    analytics.identify('dashboard-user:alice');
    analytics.group('dashboard:0x' + 'a'.repeat(40));
    assert.equal(b.requests.length, 0);
    assert.equal(b.document.cookie, '');
  }
});

test('blocked storage, malformed cookies and network failures do not break product operations', async () => {
  const b = browser('keychain.litprotocol.com', { value: 'lit_analytics_v1=not-json' });
  b.fetch = () => Promise.reject(new Error('blocked'));
  const analytics = createAnalytics('keychain', b);
  assert.doesNotThrow(() => analytics.page('secrets'));
  Object.defineProperty(b, 'document', { get() { throw new Error('blocked'); } });
  assert.doesNotThrow(() => analytics.identify('keychain-owner:alice'));
  assert.doesNotThrow(() => analytics.reset());
  await new Promise((resolve) => setImmediate(resolve));
});

test('expired sessions rotate while visitor identity persists; duplicate section effects are ignored', () => {
  const anonymous = randomUUID();
  const session = randomUUID();
  const b = browser('litprotocol.com', { value: 'lit_analytics_v1=' + encodeURIComponent(JSON.stringify({ anonymous, session, at: 0 })) });
  const analytics = createAnalytics('marketing', b);
  analytics.page('/');
  analytics.page('/');
  assert.equal(b.requests.length, 1);
  assert.equal(b.requests[0].payload.properties.distinct_id, anonymous);
  assert.notEqual(b.requests[0].payload.properties.$session_id, session);
});

test('keychain copy stays byte-identical to the shared browser client', () => {
  for (const name of ['analytics.js', 'analytics.d.ts']) {
    assert.equal(readFileSync(new URL(name, import.meta.url), 'utf8'),
      readFileSync(new URL('../lit-agent-keychain/web/src/' + name, import.meta.url), 'utf8'));
  }
});

test('docs script is regenerated from the shared client and tracks Mintlify navigation', async () => {
  const { buildDocsScript } = await import('../scripts/sync-product-analytics.mjs');
  const script = buildDocsScript();
  assert.equal(readFileSync(new URL('../docs/lit-analytics.js', import.meta.url), 'utf8'), script);
  const jar = { value: '' };
  const b = browser('developer.litprotocol.com', jar);
  const listeners = {};
  b.location.pathname = '/quickstart';
  const window = { addEventListener: (type, fn) => { listeners['window:' + type] = fn; } };
  b.document.addEventListener = (type, fn) => { listeners['document:' + type] = fn; };
  class Element { closest() { return this.link; } }
  class HTMLAnchorElement extends Element {}
  new Function('window', 'document', 'location', 'crypto', 'fetch', 'Element', 'HTMLAnchorElement', 'globalThis', script)(
    window, b.document, b.location, b.crypto, b.fetch, Element, HTMLAnchorElement,
    { document: b.document, location: b.location, crypto: b.crypto, fetch: b.fetch });
  b.location.pathname = '/lit-actions/index';
  listeners['window:mintlify:navigate']();
  const target = new Element();
  target.link = Object.assign(new HTMLAnchorElement(), { hostname: 'dashboard.chipotle.litprotocol.com' });
  listeners['document:click']({ target });
  const events = b.requests.map((r) => [r.payload.event, r.payload.properties.$pathname, r.payload.properties.destination]);
  assert.deepEqual(events, [
    ['$pageview', '/quickstart', undefined],
    ['$pageview', '/lit-actions/index', undefined],
    ['product_cta_clicked', '/lit-actions/index', 'dashboard'],
  ]);
  assert.equal(b.requests[0].payload.properties.product, 'docs');
  // Same cookie as the other litprotocol.com properties, so the journey links up.
  const dashboard = browser('dashboard.chipotle.litprotocol.com', jar);
  createAnalytics('dashboard', dashboard).page('sign-in');
  assert.equal(dashboard.requests[0].payload.properties.distinct_id, b.requests[0].payload.properties.distinct_id);
});
