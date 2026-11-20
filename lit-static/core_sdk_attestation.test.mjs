// The client must not send anything to an origin it is meant to verify until
// verification has succeeded.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createClient, AttestationError, LIT_API_URL, ATTESTED_ORIGINS, DEFAULT_ATTESTATION_TTL_MS, DEFAULT_ATTESTATION_MAX_AGE_MS } from './core_sdk.js';

function recordFetch(handler) {
  const calls = [];
  const original = globalThis.fetch;
  globalThis.fetch = async (url, init) => {
    calls.push(String(url));
    return handler(String(url), init);
  };
  return { calls, restore: () => (globalThis.fetch = original) };
}
const ok = () => Response.json({ exists: true });
// In Node.js the client asks for the attestation on a connection of its own,
// to learn the TLS key that answered. Passing `fetch` routes it through the
// recorder instead, as in a browser.
const viaFetch = { fetch: (...args) => globalThis.fetch(...args) };
const verified = (origin) => ({ origin, verifiedAt: Date.now(), platform: 'sev-snp' });

test('origins without a policy are used as before, with no attestation request', async () => {
  const net = recordFetch(ok);
  try {
    const client = createClient('https://self-hosted.example');
    assert.deepEqual(await client.accountExists('key'), { exists: true });
    assert.deepEqual(net.calls, ['https://self-hosted.example/core/v1/account_exists']);
    assert.equal(client.attestation, null);
  } finally {
    net.restore();
  }
});

test('a known origin is attested before the first request, and nothing is sent if that fails', async () => {
  assert.ok(ATTESTED_ORIGINS[LIT_API_URL]);
  const net = recordFetch((url) => (url.includes('/attestation/v3') ? new Response('', { status: 404 }) : ok()));
  try {
    const client = createClient({ baseUrl: LIT_API_URL, attestation: viaFetch });
    await assert.rejects(client.accountExists('secret-key'), (e) => e instanceof AttestationError && e.code === 'endpoint_unavailable');
    assert.equal(net.calls.length, 1);
    assert.match(net.calls[0], /^https:\/\/api2\.chipotle\.litprotocol\.com\/attestation\/v3\?nonce=[0-9a-f]{64}$/);
    assert.equal(client.attestation, null);
    // A failure is not cached: the next request verifies again.
    await assert.rejects(client.accountExists('secret-key'), AttestationError);
    assert.equal(net.calls.length, 2);
  } finally {
    net.restore();
  }
});

test('attestation can be turned off or forced on', async () => {
  const net = recordFetch((url) => (url.includes('/attestation/v3') ? new Response('', { status: 500 }) : ok()));
  try {
    await createClient({ baseUrl: LIT_API_URL, attestation: false }).accountExists('key');
    assert.deepEqual(net.calls, [`${LIT_API_URL}/core/v1/account_exists`]);
    // Forced on for an unknown origin without a policy: refused before any request.
    await assert.rejects(
      createClient({ baseUrl: 'https://self-hosted.example', attestation: true }).accountExists('key'),
      (e) => e.code === 'no_policy',
    );
    assert.equal(net.calls.length, 1);
    assert.throws(() => createClient({ baseUrl: LIT_API_URL, attestation: 'yes' }), /attestation must be/);
  } finally {
    net.restore();
  }
});

test('one verification is shared by concurrent requests and reused until it expires', async () => {
  const net = recordFetch(ok);
  try {
    const seen = [];
    const client = createClient({ baseUrl: LIT_API_URL, attestation: { ttlMs: 1000, onVerified: (r) => seen.push(r) } });
    let verifications = 0;
    client.verifyAttestation = async function () {
      verifications++;
      await new Promise((resolve) => setTimeout(resolve, 10));
      const result = verified(this.origin);
      this._attestation.result = result;
      this._attestation.onVerified?.(result);
      return result;
    };
    await Promise.all([client.accountExists('a'), client.accountExists('b'), client.accountExists('c')]);
    assert.equal(verifications, 1);
    await client.accountExists('d');
    assert.equal(verifications, 1);
    assert.equal(client.attestation.origin, LIT_API_URL);
    assert.equal(seen.length, 1);
    // Past ttlMs: the request goes out at once and the refresh runs behind it.
    client._attestation.result.verifiedAt = Date.now() - 2000;
    await client.accountExists('e');
    assert.equal(net.calls.length, 5, 'the request did not wait for the refresh');
    assert.equal(verifications, 2);
    assert.ok(client._attestation.pending, 'refresh still in flight');
    await client._attestation.pending;
    assert.ok(Date.now() - client.attestation.verifiedAt < 1000);
    // Past maxAgeMs (an idle client waking up): the request waits.
    client._attestation.maxAgeMs = 5000;
    client._attestation.result.verifiedAt = Date.now() - 6000;
    await client.accountExists('f');
    assert.equal(verifications, 3);
    assert.equal(client._attestation.pending, null, 'verification finished before the request');
    assert.ok(DEFAULT_ATTESTATION_TTL_MS >= 60_000 && DEFAULT_ATTESTATION_MAX_AGE_MS > DEFAULT_ATTESTATION_TTL_MS);
  } finally {
    net.restore();
  }
});

test('a failed background refresh stops the requests that follow', async () => {
  const net = recordFetch(ok);
  try {
    const client = createClient({ baseUrl: LIT_API_URL, attestation: { ttlMs: 1000 } });
    let fail = false;
    client.verifyAttestation = async function () {
      if (fail) throw new AttestationError('not_governed', 'release is no longer approved');
      const result = verified(this.origin);
      this._attestation.result = result;
      return result;
    };
    await client.accountExists('a');
    // The origin stops verifying. The request that triggers the refresh still
    // goes out on the attestation it holds...
    fail = true;
    client._attestation.result.verifiedAt = Date.now() - 2000;
    await client.accountExists('b');
    assert.equal(net.calls.length, 2);
    await new Promise((resolve) => setTimeout(resolve, 5));
    // ...and once the refresh has failed, nothing more is sent.
    assert.equal(client.attestation, null);
    await assert.rejects(client.accountExists('c'), (e) => e.code === 'not_governed');
    assert.equal(net.calls.length, 2);
  } finally {
    net.restore();
  }
});

test('legacy origins warn once and are not attested', async () => {
  const warnings = [];
  const originalWarn = console.warn;
  console.warn = (m) => warnings.push(m);
  const net = recordFetch(ok);
  try {
    createClient('https://api.chipotle.litprotocol.com');
    const client = createClient('https://api.chipotle.litprotocol.com/');
    await client.accountExists('key');
    assert.equal(warnings.length, 1);
    assert.match(warnings[0], /being retired.*api2\.chipotle\.litprotocol\.com/);
    assert.deepEqual(net.calls, ['https://api.chipotle.litprotocol.com/core/v1/account_exists']);
  } finally {
    console.warn = originalWarn;
    net.restore();
  }
});

test('the default client is the production API, attested, not an unauthenticated localhost', async () => {
  const net = recordFetch((url) => (url.includes('/attestation/v3') ? new Response('', { status: 404 }) : ok()));
  try {
    for (const client of [createClient(), new (createClient().constructor)()]) {
      assert.equal(client.origin, LIT_API_URL);
      await assert.rejects(client.accountExists('secret-key'), AttestationError);
    }
    assert.ok(net.calls.every((c) => c.startsWith(`${LIT_API_URL}/attestation/v3`)));
  } finally {
    net.restore();
  }
});

test('a one-off check with other options does not replace the client policy result', async () => {
  const net = recordFetch((url) => (url.includes('/attestation/v3') ? new Response('', { status: 404 }) : ok()));
  try {
    const client = createClient(LIT_API_URL);
    const held = verified(LIT_API_URL);
    client._attestation.result = held;
    // A diagnostic with a weaker policy fails or succeeds on its own terms...
    await assert.rejects(client.verifyAttestation({ tlsBinding: 'off' }), AttestationError);
    assert.equal(client.attestation, held, 'a one-off check leaves the client attestation alone');
    // ...but a failed check under the client policy withdraws it.
    await assert.rejects(client.verifyAttestation(), AttestationError);
    assert.equal(client.attestation, null);
  } finally {
    net.restore();
  }
});

test('an attested client refuses redirects', async () => {
  const seen = [];
  const original = globalThis.fetch;
  globalThis.fetch = async (url, init) => {
    seen.push(init?.redirect);
    return ok();
  };
  try {
    const client = createClient(LIT_API_URL);
    client._attestation.result = verified(LIT_API_URL);
    await client.accountExists('k');
    await createClient({ baseUrl: 'https://self-hosted.example' }).accountExists('k');
    // A redirect would carry the API key to an origin nobody verified.
    assert.deepEqual(seen, ['error', undefined]);
  } finally {
    globalThis.fetch = original;
  }
});

test('a scheme-relative base URL is attested like the origin it resolves to', () => {
  const original = globalThis.location;
  globalThis.location = { href: 'https://dashboard.example/app' };
  try {
    const client = createClient('//api2.chipotle.litprotocol.com');
    assert.equal(client.origin, LIT_API_URL);
    assert.ok(client._attestation, 'default attestation applies');
  } finally {
    globalThis.location = original;
  }
});

test('a request never falls back to an unpinned transport when a refresh fails under it', async () => {
  const net = recordFetch(ok);
  try {
    const client = createClient({ baseUrl: LIT_API_URL, attestation: { ttlMs: 0, maxAgeMs: 60_000 } });
    // Holds a TLS-bound attestation that is due a refresh; the refresh fails
    // while the request is choosing its transport.
    client._attestation.result = { origin: LIT_API_URL, verifiedAt: Date.now() - 1000, tlsBinding: 'verified', tlsKeySha256: '11'.repeat(32) };
    client.verifyAttestation = async () => {
      throw new AttestationError('policy_violation', 'no longer verifies');
    };
    let usedKey = null;
    const transport = client._transport.bind(client);
    client._transport = async (result) => {
      await new Promise((r) => setTimeout(r, 5)); // let the refresh fail first
      usedKey = result?.tlsKeySha256 ?? null;
      return async () => ok();
    };
    await client.accountExists('secret-key');
    assert.equal(usedKey, '11'.repeat(32), 'the request stays on the attestation it started with');
    assert.equal(client.attestation, null);
    assert.deepEqual(net.calls, [], 'nothing went out on the platform fetch');
    // With nothing attested there is no transport at all.
    await assert.rejects(transport(null), /no attestation/);
  } finally {
    net.restore();
  }
});

test('a relative base URL is resolved against the document base and requests go to that origin', async () => {
  const net = recordFetch((url) => (url.includes('/attestation/v3') ? new Response('', { status: 404 }) : ok()));
  const had = 'document' in globalThis;
  globalThis.document = { baseURI: `${LIT_API_URL}/app/` };
  try {
    // A page whose <base> points at the API: `fetch` would send a relative URL
    // there, so that is the origin to attest, and nothing is sent unverified.
    const client = createClient({ baseUrl: '/', attestation: viaFetch });
    assert.equal(client.origin, LIT_API_URL);
    assert.equal(client.baseUrl, `${LIT_API_URL}/core/v1`);
    await assert.rejects(client.accountExists('secret-key'), AttestationError);
    assert.equal(net.calls.length, 1);
    assert.match(net.calls[0], /\/attestation\/v3\?nonce=/);
  } finally {
    if (had) delete globalThis.document.baseURI;
    else delete globalThis.document;
    net.restore();
  }
});
