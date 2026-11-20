// TLS binding against a real TLS server on localhost. The captured report
// commits to nothing, so a full pass cannot be staged here; what can be shown
// is that evidence is fetched on fresh connections, that a file from another
// VM is asked for again, that a deployment without
// evidence is told apart from one with bad evidence, and that bad evidence
// never yields 'verified'. The pass path is exercised on every deploy by
// lit-static/sdk/scripts/verify-live.mjs.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import https from 'node:https';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { AttestationError, LIT_PRODUCTION_POLICY, LIT_STAGING_POLICY, TLS_PIN_MISMATCH, createPinnedFetch, resolvePolicy, verifyTlsBinding, LIT_API_URL } from './attestation.js';
import { createClient } from './core_sdk.js';
import { X509Certificate } from 'node:crypto';

const fixture = JSON.parse(readFileSync(new URL('./testdata/attestation-sev-snp.json', import.meta.url), 'utf8'));
const NOW = Date.UTC(2026, 10, 18, 12);
const EXPECTED = {
  appId: LIT_STAGING_POLICY.appId,
  composeHash: '6d1fd54242797994d6877fcb31d6738d00b8973aee5b073fceb30a0cf94ab2b9',
  osImageHash: 'ec43cb8c5e04b6bb6c53b33b16076a01d83d01f7f4b703f2f78a0d947d6247e6',
  reportId: 'ab'.repeat(32),
};

let openssl = true;
const dir = mkdtempSync(join(tmpdir(), 'lit-tls-'));
try {
  execFileSync('openssl', ['req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1', '-nodes', '-days', '2',
    '-subj', '/CN=localhost', '-addext', 'subjectAltName=DNS:localhost', '-keyout', join(dir, 'key.pem'), '-out', join(dir, 'cert.pem')], { stdio: 'pipe' });
} catch {
  openssl = false;
}
const key = openssl ? readFileSync(join(dir, 'key.pem')) : null;
const cert = openssl ? readFileSync(join(dir, 'cert.pem')) : null;
rmSync(dir, { recursive: true, force: true });

async function withServer(files, run) {
  const connections = new Set();
  const requestsPerConnection = [];
  const server = https.createServer({ key, cert }, (req, res) => {
    requestsPerConnection.push(req.socket);
    const body = files[req.url];
    res.writeHead(body === undefined ? 404 : 200);
    res.end(body ?? '');
  });
  server.on('secureConnection', (socket) => connections.add(socket));
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try {
    return await run(`https://localhost:${server.address().port}`, { connections, requestsPerConnection });
  } finally {
    server.closeAllConnections?.();
    await new Promise((resolve) => server.close(resolve));
  }
}
const options = () => ({ tls: { ca: cert }, collateral: { vcek: fixture.vcek, ask: fixture.ask }, now: NOW, timeoutMs: 5000 });
const manifest = () => `${createHash('sha256').update(cert).digest('hex')}  cert-localhost.pem\n`;

test('built-in policies require TLS binding where the runtime can check it', () => {
  assert.equal(LIT_PRODUCTION_POLICY.tlsBinding, 'require');
  assert.equal(resolvePolicy(LIT_API_URL).policy.tlsBinding, 'require');
  assert.equal(resolvePolicy(LIT_API_URL, { tlsBinding: 'off' }).policy.tlsBinding, 'off');
});

test('a binding that must hold is refused for a misspelt mode or a plain-HTTP origin', () => {
  const explicit = { appId: 'aa'.repeat(20), kmsContract: '0x' + 'bb'.repeat(20) };
  // A typo must not quietly become best-effort.
  assert.throws(() => resolvePolicy(LIT_API_URL, { tlsBinding: 'required' }), (e) => e.code === 'invalid_argument');
  for (const mode of ['require', 'strict']) {
    assert.throws(() => resolvePolicy('http://lit.example', { ...explicit, tlsBinding: mode }), (e) => e.code === 'invalid_argument');
  }
  assert.equal(resolvePolicy('http://localhost:8000', { ...explicit, tlsBinding: 'auto' }).policy.tlsBinding, 'auto');
});

test('a deployment that publishes no evidence is reported as unavailable', { skip: !openssl }, async () => {
  await withServer({}, async (origin) => {
    assert.deepEqual(await verifyTlsBinding(origin, EXPECTED, options()), { status: 'unavailable' });
  });
  assert.deepEqual(await verifyTlsBinding('http://localhost:1', EXPECTED, options()), { status: 'unsupported' });
  // Without the identity of the VM that answered, there is nothing to bind to.
  await assert.rejects(verifyTlsBinding('https://localhost:1', { ...EXPECTED, reportId: undefined }, options()), (e) => e.code === 'invalid_argument');
});

test('a file answered by another VM is asked for again, a bounded number of times', { skip: !openssl }, async () => {
  const files = {
    '/evidences/attestation.json': JSON.stringify({ attestation: fixture.attestation }),
    '/evidences/sha256sum.txt': manifest(),
    '/evidences/cert-localhost.pem': cert,
  };
  // The main attestation came over this server's key: one request per file.
  await withServer(files, async (origin, seen) => {
    await assert.rejects(verifyTlsBinding(origin, EXPECTED, { ...options(), tlsKeySha256: keyHash() }), AttestationError);
    assert.equal(seen.requestsPerConnection.length, 3);
  });
  // It came over a key this server never presents, as when the VM that
  // answered has since stopped: each file is retried, then the binding fails.
  await withServer(files, async (origin, seen) => {
    await assert.rejects(verifyTlsBinding(origin, EXPECTED, { ...options(), tlsKeySha256: '22'.repeat(32) }), (e) => e.code === 'tls_binding_failed');
    assert.equal(seen.requestsPerConnection.length, 30);
    assert.equal(new Set(seen.requestsPerConnection).size, 30);
  });
});

test('each evidence file is fetched on its own connection', { skip: !openssl }, async () => {
  const files = {
    '/evidences/attestation.json': JSON.stringify({ attestation: fixture.attestation }),
    '/evidences/sha256sum.txt': manifest(),
    '/evidences/cert-localhost.pem': cert,
  };
  await withServer(files, async (origin, seen) => {
    await assert.rejects(verifyTlsBinding(origin, EXPECTED, options()), AttestationError);
    assert.equal(seen.requestsPerConnection.length, 3);
    // The ingress routes /evidences by a connection's first bytes, so no
    // connection may carry more than one request.
    assert.equal(new Set(seen.requestsPerConnection).size, 3);
  });
});

test('evidence that does not commit to the certificate never verifies', { skip: !openssl }, async () => {
  const good = {
    '/evidences/attestation.json': JSON.stringify({ attestation: fixture.attestation }),
    '/evidences/sha256sum.txt': manifest(),
    '/evidences/cert-localhost.pem': cert,
  };
  const failed = (files, pattern) =>
    withServer(files, (origin) =>
      assert.rejects(verifyTlsBinding(origin, EXPECTED, options()), (e) => {
        assert.equal(e.code, 'tls_binding_failed', e.message);
        if (pattern) assert.match(e.message, pattern);
        return true;
      }),
    );
  // A genuine report for this app, but one that commits to nothing: replaying
  // the main attestation as certificate evidence must not pass.
  await failed(good, /did not verify/);
  await failed({ ...good, '/evidences/attestation.json': 'not json' }, /not JSON/);
  await failed({ ...good, '/evidences/attestation.json': JSON.stringify({ attestation: '000000' }) }, /did not verify/);
  // Evidence present but incomplete is a failure, not "unavailable".
  await failed({ '/evidences/attestation.json': good['/evidences/attestation.json'] }, /HTTP 404/);
});

test('a server the client cannot authenticate is a failure, not a skip', { skip: !openssl }, async () => {
  await withServer({}, async (origin) => {
    await assert.rejects(verifyTlsBinding(origin, EXPECTED, { ...options(), tls: undefined }), (e) => e.code === 'tls_binding_failed');
  });
});

const keyHash = () => createHash('sha256').update(new X509Certificate(cert).publicKey.export({ type: 'spki', format: 'der' })).digest('hex');

test('a pinned connection reaches only the attested key', { skip: !openssl }, async () => {
  await withServer({ '/core/v1/account_exists': '{"exists":true}' }, async (origin, seen) => {
    const pinned = await createPinnedFetch(keyHash(), { tls: { ca: cert } });
    const res = await pinned(`${origin}/core/v1/account_exists`, { method: 'POST', headers: { 'X-Api-Key': 'k', 'Content-Type': 'application/json' }, body: '{}' });
    assert.equal(res.status, 200);
    assert.deepEqual(await res.json(), { exists: true });
    assert.equal((await pinned(`${origin}/missing`)).status, 404);
    const sent = seen.requestsPerConnection.length;

    // Same hostname, valid certificate, different key: the request is never sent.
    const other = await createPinnedFetch('00'.repeat(32), { tls: { ca: cert } });
    await assert.rejects(other(`${origin}/core/v1/account_exists`, { method: 'POST', body: 'secret' }), (e) => e.code === TLS_PIN_MISMATCH);
    assert.equal(seen.requestsPerConnection.length, sent, 'no request reached the server');
    await assert.rejects(pinned('http://localhost/'), TypeError);
  });
  await assert.rejects(createPinnedFetch('not-a-hash'), (e) => e.code === 'invalid_argument');
  await assert.rejects(createPinnedFetch([]), (e) => e.code === 'invalid_argument');
});

test('a pin may hold the keys of several verified VMs, and a refusal names the key presented', { skip: !openssl }, async () => {
  await withServer({ '/core/v1/account_exists': '{"exists":true}' }, async (origin) => {
    const either = await createPinnedFetch(['00'.repeat(32), keyHash()], { tls: { ca: cert } });
    assert.equal((await either(`${origin}/core/v1/account_exists`)).status, 200);
    const neither = await createPinnedFetch(['00'.repeat(32), '11'.repeat(32)], { tls: { ca: cert } });
    await assert.rejects(neither(`${origin}/core/v1/account_exists`), (e) => e.code === TLS_PIN_MISMATCH && e.tlsKeySha256 === keyHash());
  });
});

test('during a cutover the client keeps the keys of both VMs it verified', { skip: !openssl }, async () => {
  await withServer({ '/core/v1/account_exists': '{"exists":true}' }, async (origin, seen) => {
    const client = createClient({ baseUrl: origin, attestation: { appId: 'aa'.repeat(20), kmsContract: '0x' + 'bb'.repeat(20), tls: { ca: cert } } });
    const state = client._attestation;
    const aims = [];
    const other = '11'.repeat(32); // the VM this server is not
    let next = other;
    let onlyOther = false;
    client.verifyAttestation = async function () {
      aims.push(state.aimTlsKeySha256 ?? null);
      state.aimTlsKeySha256 = null;
      const result = { origin, verifiedAt: Date.now(), tlsBinding: 'verified', tlsKeySha256: next };
      state.result = result;
      (state.keys ??= new Map()).set(next, result.verifiedAt);
      if (!onlyOther) next = keyHash();
      return result;
    };
    assert.deepEqual(await client.accountExists('k'), { exists: true });
    // The refused connection named its key, and that VM was verified next.
    assert.deepEqual(aims, [null, keyHash()]);
    assert.equal(seen.requestsPerConnection.length, 1);
    // A later verification that lands on the other VM again changes nothing:
    // this server's key is still pinned, so requests keep going without one.
    state.result = { origin, verifiedAt: Date.now(), tlsBinding: 'verified', tlsKeySha256: other };
    assert.deepEqual(await client.accountExists('k'), { exists: true });
    assert.equal(aims.length, 2);
    // A key verified longer ago than maxAgeMs is dropped from the pin.
    // A key verified longer ago than maxAgeMs is dropped from the pin, and
    // the client gives up after a bounded number of verifications.
    state.keys.set(keyHash(), Date.now() - state.maxAgeMs - 1);
    next = other;
    onlyOther = true;
    await assert.rejects(client.accountExists('k'), (e) => e.code === TLS_PIN_MISMATCH);
    assert.equal(aims.length, 5);
    assert.equal(seen.requestsPerConnection.length, 2);
  });
});

test('the client pins its API requests and re-verifies when the key changes', { skip: !openssl }, async () => {
  await withServer({ '/core/v1/account_exists': '{"exists":true}' }, async (origin, seen) => {
    const client = createClient({ baseUrl: origin, attestation: { appId: 'aa'.repeat(20), kmsContract: '0x' + 'bb'.repeat(20), tls: { ca: cert } } });
    let verifications = 0;
    let attestedKey = '11'.repeat(32); // a key the server does not hold
    client.verifyAttestation = async function () {
      verifications++;
      const result = { origin, verifiedAt: Date.now(), tlsBinding: 'verified', tlsKeySha256: attestedKey };
      this._attestation.result = result;
      attestedKey = keyHash(); // the next verification attests the real key
      return result;
    };
    // First attempt is pinned to the wrong key and sends nothing; the client
    // verifies again and the retry goes out on the attested key.
    assert.deepEqual(await client.accountExists('k'), { exists: true });
    assert.equal(verifications, 2);
    assert.equal(seen.requestsPerConnection.length, 1);
    assert.equal(client.attestation.tlsKeySha256, keyHash());
    await client.accountExists('k');
    assert.equal(verifications, 2);
  });
});

test('a pin still applies when certificate rejection is turned off', { skip: !openssl }, async () => {
  await withServer({ '/core/v1/account_exists': '{"exists":true}' }, async (origin, seen) => {
    // Node skips `checkServerIdentity` failures when unauthorized peers are
    // accepted, which would turn the pin into a no-op.
    const previous = process.env.NODE_TLS_REJECT_UNAUTHORIZED;
    process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';
    try {
      const other = await createPinnedFetch('00'.repeat(32), { tls: { ca: cert, rejectUnauthorized: false } });
      await assert.rejects(other(`${origin}/core/v1/account_exists`, { method: 'POST', body: 'secret' }), (e) => e.code === TLS_PIN_MISMATCH);
      assert.equal(seen.requestsPerConnection.length, 0, 'no request reached the server');
    } finally {
      if (previous === undefined) delete process.env.NODE_TLS_REJECT_UNAUTHORIZED;
      else process.env.NODE_TLS_REJECT_UNAUTHORIZED = previous;
    }
  });
});
