// Runs against real evidence: an AMD SEV-SNP attestation captured from the
// staging deployment, with the VCEK and ASK AMD issued for that chip. Nothing
// here is mocked below the HTTP layer, so a pass means the verifier accepts
// what real hardware produces and rejects every single-field tamper of it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import {
  AttestationError,
  ATTESTED_ORIGINS,
  LEGACY_ORIGINS,
  LIT_API_URL,
  LIT_STAGING_POLICY,
  amdKdsVcekUrl,
  attestationV3ReportData,
  computeSnpLaunch,
  decodeAttestation,
  keccak256,
  parseSnpReport,
  resolvePolicy,
  verifyAttestation,
  verifySnpEvidence,
} from './attestation.js';

const fixture = JSON.parse(await readFile(new URL('./testdata/attestation-sev-snp.json', import.meta.url), 'utf8'));
const hex = (h) => Uint8Array.from(h.match(/../g) ?? [], (x) => parseInt(x, 16));
const toHex = (b) => Buffer.from(b).toString('hex');
const ZERO_REPORT_DATA = new Uint8Array(64);
const collateral = { vcek: fixture.vcek, ask: fixture.ask };
// The VCEK was issued 2026-11-17; pin the clock so the test does not age out.
const NOW = Date.UTC(2026, 10, 18, 12);
const evidence = () => decodeAttestation(hex(fixture.attestation));
const verify = (ev = evidence(), options = {}) =>
  verifySnpEvidence(ev, { expectedReportData: ZERO_REPORT_DATA, collateral, now: NOW, ...options });
const rejects = (promise, code) =>
  assert.rejects(promise, (e) => {
    assert.ok(e instanceof AttestationError, `expected AttestationError, got ${e}`);
    assert.equal(e.code, code, e.message);
    return true;
  });

test('keccak256 matches known vectors', () => {
  assert.equal(toHex(keccak256(new Uint8Array())), 'c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470');
  assert.equal(toHex(keccak256(new TextEncoder().encode('abc'))), '4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45');
  // Spans two 136-byte blocks; cross-checked against @noble/hashes.
  assert.equal(
    toHex(keccak256(new Uint8Array(200).map((_, i) => (i * 7) & 255))),
    'e04f7ba958b8ea97baa335903e44841720f241b5b3ac011b42bd5ba61d2f085b',
  );
});

test('v3 challenge matches the server and deployment-gate vector', () => {
  // Same vector as lit-api-server attestation_v3.rs and `cast keccak`.
  const data = attestationV3ReportData(new Uint8Array(32).fill(0x11));
  assert.equal(toHex(data.subarray(0, 32)), 'c261434695b165c2ad0b967e1642f4a15def329106a93d1ba9c6e4e43ad7db7f');
  assert.deepEqual(data.subarray(32), new Uint8Array(32));
  assert.throws(() => attestationV3ReportData(new Uint8Array(31)), AttestationError);
});

test('real SEV-SNP evidence verifies end to end offline', async () => {
  const result = await verify();
  assert.equal(result.platform, 'sev-snp');
  assert.equal(result.appId, LIT_STAGING_POLICY.appId);
  assert.equal(result.composeHash, '6d1fd54242797994d6877fcb31d6738d00b8973aee5b073fceb30a0cf94ab2b9');
  assert.equal(result.osImageHash, 'ec43cb8c5e04b6bb6c53b33b16076a01d83d01f7f4b703f2f78a0d947d6247e6');
  // The dstack device id the deployment reports at /info.
  assert.equal(result.deviceId, '72791b7add93ac1f84c9d20ed738a3a091f90b92bf67914e48069086378ee9d5');
  assert.equal(result.product, 'Milan');
  assert.equal(result.tcbStatus, 'UpToDate');
  assert.deepEqual(result.tcb.reported, { bootloader: 4, tee: 0, snp: 24, microcode: 222 });
});

test('collateral is accepted as DER, hex or PEM', async () => {
  const pem = (h) => `-----BEGIN CERTIFICATE-----\n${Buffer.from(h, 'hex').toString('base64')}\n-----END CERTIFICATE-----\n`;
  await verify(evidence(), { collateral: { vcek: hex(fixture.vcek), ask: hex(fixture.ask) } });
  await verify(evidence(), { collateral: { vcek: pem(fixture.vcek), ask: pem(fixture.ask) } });
  await rejects(verify(evidence(), { collateral: null }), 'collateral_unavailable');
  await rejects(verify(evidence(), { collateral: { vcek: fixture.vcek } }), 'collateral_unavailable');
});

test('launch measurement is reproduced from the declared boot inputs', async () => {
  const ev = evidence();
  const launch = await computeSnpLaunch(ev.config);
  assert.deepEqual(launch.measurement, parseSnpReport(ev.report).measurement);
  assert.match(launch.cmdline, /dstack\.rootfs_hash=[0-9a-f]{64}/);
  assert.equal(launch.vcpus, 4);
});

test('a report for another request is stale', async () => {
  const other = new Uint8Array(64);
  other[0] = 1;
  await rejects(verify(evidence(), { expectedReportData: other }), 'stale_evidence');
  await rejects(verify(evidence(), { expectedReportData: attestationV3ReportData(new Uint8Array(32)) }), 'stale_evidence');
});

test('every signed report field is covered by the signature', async () => {
  // Offsets: policy, report_data, measurement, host_data, reported_tcb, chip_id,
  // and the signature itself.
  for (const offset of [0x08, 0x0a, 0x50, 0x90, 0xc0, 0x180, 0x1a0, 0x2a0, 0x2a0 + 72]) {
    const ev = evidence();
    ev.report = ev.report.slice();
    ev.report[offset] ^= 0x01;
    await assert.rejects(verify(ev), AttestationError, `byte ${offset.toString(16)}`);
  }
  const ev = evidence();
  ev.report = ev.report.slice(0, 1183);
  await rejects(verify(ev), 'malformed_evidence');
});

test('certificates that do not chain to the pinned AMD root are refused', async () => {
  const flip = (h, at) => {
    const b = hex(h);
    b[b.length - at] ^= 0x01;
    return b;
  };
  // Signature bytes of each certificate.
  await rejects(verify(evidence(), { collateral: { vcek: flip(fixture.vcek, 10), ask: fixture.ask } }), 'untrusted_hardware');
  await rejects(verify(evidence(), { collateral: { vcek: fixture.vcek, ask: flip(fixture.ask, 10) } }), 'untrusted_hardware');
  // A genuine AMD root is not an intermediate for this VCEK.
  await rejects(verify(evidence(), { collateral: { vcek: fixture.vcek, ask: fixture.ark } }), 'untrusted_hardware');
  // The VCEK cannot stand in for the ASK.
  await rejects(verify(evidence(), { collateral: { vcek: fixture.vcek, ask: fixture.vcek } }), 'untrusted_hardware');
});

test('certificate validity is enforced', async () => {
  await rejects(verify(evidence(), { now: Date.UTC(2026, 0, 1) }), 'untrusted_hardware');
  await rejects(verify(evidence(), { now: Date.UTC(2050, 0, 1) }), 'untrusted_hardware');
});

test('tampered launch inputs do not reproduce the measurement', async () => {
  const mutateConfig = (fn) => {
    const ev = evidence();
    const config = JSON.parse(ev.config);
    fn(config);
    ev.config = JSON.stringify(config);
    return ev;
  };
  const mutateMeasurementDoc = (fn) =>
    mutateConfig((config) => {
      const doc = JSON.parse(config.sev_snp_measurement);
      fn(doc);
      config.sev_snp_measurement = JSON.stringify(doc);
    });
  await rejects(verify(mutateMeasurementDoc((d) => (d.vcpus = 2))), 'measurement_mismatch');
  await rejects(verify(mutateMeasurementDoc((d) => (d.vcpu_type = 'EPYC-Milan'))), 'measurement_mismatch');
  // DebugSwap would expose guest registers to the host.
  await rejects(verify(mutateMeasurementDoc((d) => (d.guest_features = 0x21))), 'policy_violation');
  await rejects(verify(mutateMeasurementDoc((d) => (d.extra = 1))), 'malformed_evidence');
  // A different OS image hash no longer matches the checksum file.
  await rejects(verify(mutateConfig((c) => (c.os_image_hash = '00'.repeat(32)))), 'measurement_mismatch');
  // Swapped measurement material is not the one the image lists.
  await rejects(
    verify(
      mutateMeasurementDoc((d) => {
        const bytes = Buffer.from(d.measurement, 'base64');
        bytes[bytes.length - 1] ^= 1;
        d.measurement = bytes.toString('base64');
      }),
    ),
    'measurement_mismatch',
  );
  await rejects(verify(mutateConfig((c) => delete c.sev_snp_measurement)), 'malformed_evidence');
});

test('app identity must be the document the report binds', async () => {
  const withMrConfig = (fn) => {
    const ev = evidence();
    const doc = JSON.parse(ev.mrConfig);
    fn(doc);
    ev.mrConfig = JSON.stringify(doc);
    // Keep the unsigned vm_config copy consistent so only HOST_DATA can object.
    const config = JSON.parse(ev.config);
    config.mr_config = ev.mrConfig;
    ev.config = JSON.stringify(config);
    return ev;
  };
  await rejects(verify(withMrConfig((d) => (d.app_id = '11'.repeat(20)))), 'identity_mismatch');
  await rejects(verify(withMrConfig((d) => (d.compose_hash = '22'.repeat(32)))), 'identity_mismatch');
  await rejects(verify(withMrConfig((d) => (d.key_provider = 'none'))), 'identity_mismatch');
  const ev = evidence();
  ev.mrConfig = ev.mrConfig.replace('969a', '969b');
  await rejects(verify(ev), 'identity_mismatch');
});

test('envelope decoding fails closed', () => {
  const code = (bytes) => {
    try {
      decodeAttestation(bytes);
    } catch (e) {
      return e.code;
    }
    return 'accepted';
  };
  assert.equal(code(new Uint8Array()), 'malformed_evidence');
  assert.equal(code(Uint8Array.of(0xff)), 'malformed_evidence');
  assert.equal(code(Uint8Array.of(0x00, 0x09)), 'unsupported_platform');
  const bytes = hex(fixture.attestation);
  assert.equal(code(bytes.subarray(0, bytes.length - 1)), 'malformed_evidence');
  assert.equal(code(Uint8Array.of(...bytes, 0)), 'malformed_evidence');
  // Intel TDX evidence is recognised, never mistaken for SEV-SNP.
  assert.deepEqual(decodeAttestation(Uint8Array.of(0x00, 0x00, 0x00)), { platform: 'tdx' });
});

// The current dstack schema is MessagePack. Re-encode the captured evidence in
// it, with byte vectors both as binary strings and as serde's integer arrays.
function msgpackEnvelope(ev, bytesAsArray, certChain = []) {
  const parts = [];
  const push = (...b) => parts.push(Uint8Array.from(b));
  const str = (s) => {
    const b = new TextEncoder().encode(s);
    if (b.length < 32) push(0xa0 | b.length);
    else if (b.length < 256) push(0xd9, b.length);
    else push(0xda, b.length >> 8, b.length & 0xff);
    parts.push(b);
  };
  const bin = (b) => {
    if (bytesAsArray) {
      push(0xdc, b.length >> 8, b.length & 0xff);
      for (const x of b) x < 0x80 ? push(x) : push(0xcc, x);
    } else {
      push(0xc5, b.length >> 8, b.length & 0xff);
      parts.push(b);
    }
  };
  push(0x83);
  str('version'); push(0x01);
  str('platform'); push(0x82); str('kind'); str('sev-snp'); str('data'); push(0x83);
  str('report'); bin(ev.report); str('cert_chain'); push(0x90 | certChain.length); certChain.forEach((c) => bin(c)); str('mr_config'); str(ev.mrConfig);
  str('stack'); push(0x82); str('kind'); str('dstack'); str('data'); push(0x83);
  str('report_data'); bin(new Uint8Array(64)); str('runtime_events'); push(0x90); str('config'); str(ev.config);
  return Buffer.concat(parts);
}

test('MessagePack envelopes decode to the same evidence', async () => {
  for (const bytesAsArray of [false, true]) {
    const decoded = decodeAttestation(new Uint8Array(msgpackEnvelope(evidence(), bytesAsArray)));
    assert.deepEqual(decoded.report, evidence().report);
    assert.equal(decoded.config, evidence().config);
    assert.equal((await verify(decoded)).appId, LIT_STAGING_POLICY.appId);
  }
});

test('contradictory or incomplete envelopes are refused', async () => {
  const good = new Uint8Array(msgpackEnvelope(evidence(), false));
  const code = (bytes) => {
    try {
      decodeAttestation(new Uint8Array(bytes));
    } catch (e) {
      return e.code;
    }
    return 'accepted';
  };
  assert.equal(code(good), 'accepted');
  // A key given twice would let two readers disagree about which value counts.
  const duplicated = Buffer.concat([Buffer.from([0x84]), Buffer.from(good.subarray(1)), Buffer.from('a776657273696f6e01', 'hex')]);
  assert.equal(code(duplicated), 'malformed_evidence');
  // An unknown stack is not silently treated as dstack.
  const otherStack = Buffer.from(good);
  const at = otherStack.indexOf(Buffer.from('a664737461636b', 'hex')); // fixstr "dstack"
  assert.ok(at > 0);
  otherStack[at + 6] = 0x78;
  assert.equal(code(otherStack), 'malformed_evidence');

  // The VM's own configuration must carry the identity document too.
  const ev = evidence();
  const config = JSON.parse(ev.config);
  delete config.mr_config;
  ev.config = JSON.stringify(config);
  await rejects(verify(ev), 'identity_mismatch');
});

test('ASK and VCEK are found in a kernel certificate table', async () => {
  const vcek = hex(fixture.vcek);
  const ask = hex(fixture.ask);
  const guid = (h) => hex(h);
  const table = new Uint8Array(72 + vcek.length + ask.length);
  const view = new DataView(table.buffer);
  table.set(guid('63da758de6644564adc5f4b93be8accd'), 0);
  view.setUint32(16, 72, true);
  view.setUint32(20, vcek.length, true);
  table.set(guid('4ab7b379bbac4fe4a02f05aef327c782'), 24);
  view.setUint32(40, 72 + vcek.length, true);
  view.setUint32(44, ask.length, true);
  table.set(vcek, 72);
  table.set(ask, 72 + vcek.length);
  // No collateral in the response and AMD unreachable: the envelope's own table is what verifies.
  const ev = evidence();
  const withTable = new Uint8Array(msgpackEnvelope(ev, false, [table]));
  const { fetch, calls } = server({ v3: (nonce) => Response.json({ version: 3, nonce, attestation: toHex(withTable) }), kds: false });
  await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch, now: NOW }), 'stale_evidence');
  assert.ok(!calls.some((c) => c.includes('kdsintf.amd.com')));
});

test('KDS URL binds the chip and its reported firmware', () => {
  const url = amdKdsVcekUrl(parseSnpReport(evidence().report));
  assert.match(url, /^https:\/\/kdsintf\.amd\.com\/vcek\/v1\/Milan\/cc618c5b[0-9a-f]{120}\?blSPL=4&teeSPL=0&snpSPL=24&ucodeSPL=222$/);
});

test('policies resolve from known origins and reject legacy ones', () => {
  assert.equal(resolvePolicy(`${LIT_API_URL}/core/v1`).policy.appId, ATTESTED_ORIGINS[LIT_API_URL].appId);
  assert.equal(resolvePolicy('https://example.com', { appId: 'aa'.repeat(20), kmsContract: '0x' + 'bb'.repeat(20) }).policy.tlsBinding, 'auto');
  for (const legacy of Object.keys(LEGACY_ORIGINS)) {
    assert.throws(() => resolvePolicy(legacy), (e) => e.code === 'no_policy' && e.message.includes(LEGACY_ORIGINS[legacy]));
  }
  assert.throws(() => resolvePolicy('https://example.com'), (e) => e.code === 'no_policy');
  assert.throws(() => resolvePolicy('not a url'), (e) => e.code === 'invalid_argument');
});

// --- verifyAttestation over a mocked HTTP layer ---------------------------

const ORIGIN = 'https://lit.example';
const POLICY = { appId: LIT_STAGING_POLICY.appId, kmsContract: LIT_STAGING_POLICY.kmsContract, rpcUrls: ['https://rpc.example'], tlsBinding: 'off' };
const TRUE = '0x' + '0'.repeat(63) + '1';
const FALSE = '0x' + '0'.repeat(64);

function server({ v3, rpc = () => TRUE, kds = true } = {}) {
  const calls = [];
  const fetch = async (url, init = {}) => {
    const u = new URL(url);
    calls.push(u.href);
    if (u.pathname === '/attestation/v3') return v3(u.searchParams.get('nonce'), calls);
    if (u.host === 'rpc.example') {
      const call = JSON.parse(init.body).params[0];
      return Response.json({ jsonrpc: '2.0', id: 1, result: rpc(call) });
    }
    if (u.host === 'kdsintf.amd.com') {
      if (!kds) return new Response('', { status: 429 });
      if (u.pathname.endsWith('/cert_chain')) {
        const pem = (h) => `-----BEGIN CERTIFICATE-----\n${Buffer.from(h, 'hex').toString('base64')}\n-----END CERTIFICATE-----\n`;
        return new Response(pem(fixture.ask) + pem(fixture.ark));
      }
      return new Response(hex(fixture.vcek));
    }
    return new Response('', { status: 404 });
  };
  return { fetch, calls };
}
const v3Body = (nonce, extra = {}) =>
  Response.json({ version: 3, nonce, report_data: toHex(attestationV3ReportData(hex(nonce))), attestation: fixture.attestation, ...extra });

test('a replayed report is rejected even though the JSON echoes are fresh', async () => {
  // The captured report is genuine and its echoes are recomputed for this
  // request, but the signed report data belongs to another one.
  const { fetch, calls } = server({ v3: (nonce) => v3Body(nonce, { collateral }) });
  await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch, now: NOW }), 'stale_evidence');
  assert.match(calls[0], /\/attestation\/v3\?nonce=[0-9a-f]{64}$/);
  assert.ok(!calls.some((c) => c.includes('rpc.example')), 'governance must not be consulted for unverified evidence');
});

test('each request uses a fresh random nonce', async () => {
  const nonces = new Set();
  const { fetch } = server({ v3: (nonce) => (nonces.add(nonce), v3Body(nonce, { collateral })) });
  for (let i = 0; i < 3; i++) await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch, now: NOW }), 'stale_evidence');
  assert.equal(nonces.size, 3);
});

test('collateral falls back to AMD KDS and reports when it is unreachable', async () => {
  const withKds = server({ v3: (nonce) => v3Body(nonce) });
  await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch: withKds.fetch, now: NOW }), 'stale_evidence');
  assert.ok(withKds.calls.some((c) => c.startsWith('https://kdsintf.amd.com/vcek/v1/Milan/cc618c5b')));
  assert.ok(withKds.calls.some((c) => c === 'https://kdsintf.amd.com/vcek/v1/Milan/cert_chain'));
  const withoutKds = server({ v3: (nonce) => v3Body(nonce), kds: false });
  await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch: withoutKds.fetch, now: NOW }), 'collateral_unavailable');
});

test('bad collateral from the deployment falls back to AMD instead of failing', async () => {
  // A VCEK cached before a firmware update, or plain garbage, must not be
  // worse than no collateral: the verifier moves on to AMD's own copy.
  for (const bad of [{ vcek: fixture.ask, ask: fixture.ask }, { vcek: '00', ask: 'zz' }]) {
    const { fetch, calls } = server({ v3: (nonce) => v3Body(nonce, { collateral: bad }) });
    // Reaching stale_evidence means the certificate chain verified via KDS.
    await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch, now: NOW }), 'stale_evidence');
    assert.ok(calls.some((c) => c.startsWith('https://kdsintf.amd.com/vcek/v1/Milan/')));
  }
  // With AMD unreachable too, the certificate problem is what gets reported.
  const { fetch } = server({ v3: (nonce) => v3Body(nonce, { collateral: { vcek: fixture.ask, ask: fixture.ask } }), kds: false });
  await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch, now: NOW }), 'untrusted_hardware');
});

test('collateral that is not even a certificate falls back too', async () => {
  // Truncated DER used to raise a raw TypeError and stop the fallback.
  for (const vcek of ['30', '3003300030', fixture.vcek.slice(0, 40)]) {
    const { fetch, calls } = server({ v3: (nonce) => v3Body(nonce, { collateral: { vcek, ask: fixture.ask } }) });
    await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch, now: NOW }), 'stale_evidence');
    assert.ok(calls.some((c) => c.startsWith('https://kdsintf.amd.com/')));
  }
});

test('a busy endpoint is retried, a broken one is reported', async () => {
  let attempts = 0;
  const busy = server({ v3: (nonce) => (++attempts < 3 ? new Response('', { status: 429 }) : v3Body(nonce, { collateral })) });
  await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch: busy.fetch, now: NOW }), 'stale_evidence');
  assert.equal(attempts, 3);
  const missing = server({ v3: () => new Response('', { status: 404 }) });
  await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch: missing.fetch, now: NOW }), 'endpoint_unavailable');
  const wrong = server({ v3: (nonce) => Response.json({ version: 2, nonce }) });
  await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch: wrong.fetch, now: NOW }), 'malformed_evidence');
  const tdx = server({ v3: (nonce) => Response.json({ version: 3, nonce, attestation: '000000' }) });
  await rejects(verifyAttestation(ORIGIN, { ...POLICY, fetch: tdx.fetch, now: NOW }), 'unsupported_platform');
});

test('firmware currency is opt-in on the client', async () => {
  // The app's on-chain requireTcbUpToDate gates key release at boot, so the
  // built-in policies do not repeat it; a caller can ask for it.
  assert.equal(resolvePolicy(LIT_API_URL).policy.requireUpToDateTcb, undefined);
  assert.equal(LIT_STAGING_POLICY.requireUpToDateTcb, undefined);
  assert.equal(resolvePolicy(LIT_API_URL, { requireUpToDateTcb: true }).policy.requireUpToDateTcb, true);
});

test('unknown origins need an explicit policy', async () => {
  const { fetch, calls } = server({ v3: (nonce) => v3Body(nonce) });
  await rejects(verifyAttestation(ORIGIN, { fetch }), 'no_policy');
  await rejects(verifyAttestation('https://api.chipotle.litprotocol.com', { fetch }), 'no_policy');
  assert.equal(calls.length, 0);
});
