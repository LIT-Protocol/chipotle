/**
 * Lit Chipotle verification UI.
 *
 * Vanilla JS + ethers.js v6.13.0. Performs the quick-verify checks from
 * https://developer.litprotocol.com/architecture/verification/quick-verify
 * entirely in the browser:
 *   1. Read the live attestation (GET /info, GET /attestation).
 *   2. Recompute SHA-256(app_compose) and compare to the reported compose_hash.
 *   3. Confirm the server's app_id matches the expected DstackApp address.
 *   4. Query allowedComposeHashes(bytes32) on the DstackApp contract on Base.
 * Step 5 (Intel DCAP quote validation + TLS-in-TEE pinning) needs introspection
 * a browser cannot do, so it defers to the Phala Trust Center.
 */

const DEFAULTS = {
  apiUrl: 'https://api.chipotle.litprotocol.com',
  rpcUrl: 'https://mainnet.base.org',
  // The on-chain DstackApp contract that whitelists Lit Chipotle's docker-compose
  // configurations. Its address (lowercased, no 0x) is also the dstack app_id.
  appId: '0x3F91Deaf16FF7C823eE65081d6bAFA1cEea05FfC',
};

// Reference contracts on Base, all administered by the Lit Safe multisig.
const CONTRACTS = {
  kms: '0x2f83172A49584C017F2B256F0FB2Dca14126Ba9C',
  safe: '0xF688411c0FFc300cAb33EB1dA651DBb3E6891098',
};

const DSTACK_APP_ABI = ['function allowedComposeHashes(bytes32) view returns (bool)'];

// ── DOM helpers ──────────────────────────────────────────────────────────────

const el = (id) => document.getElementById(id);
const pillOf = (stepId) => el(stepId).querySelector('[data-pill]');
const detailOf = (stepId) => el(stepId).querySelector('[data-detail]');

function setPill(stepId, status, text) {
  const pill = pillOf(stepId);
  pill.className = 'pill ' + status;
  pill.textContent = text;
}

function setDetail(stepId, html) {
  detailOf(stepId).innerHTML = html;
}

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function kv(label, value, cls) {
  const v = value === undefined || value === null || value === '' ? '—' : escapeHtml(value);
  return `<div class="kv"><span class="k">${escapeHtml(label)}</span><span class="v ${cls || ''}">${v}</span></div>`;
}

function link(href, text) {
  return `<a href="${escapeHtml(href)}" target="_blank" rel="noopener noreferrer">${escapeHtml(text)}</a>`;
}

function collapsible(summary, body) {
  return `<details><summary>${escapeHtml(summary)}</summary><pre>${escapeHtml(body)}</pre></details>`;
}

// ── Crypto / formatting helpers ──────────────────────────────────────────────

/** Lowercase hex, no 0x prefix. */
function normHex(h) {
  return String(h || '')
    .trim()
    .toLowerCase()
    .replace(/^0x/, '');
}

async function sha256hex(str) {
  if (!(window.crypto && window.crypto.subtle)) {
    throw new Error(
      'Web Crypto (crypto.subtle) is unavailable. Open this page over https:// or http://localhost — not file://.'
    );
  }
  const data = new TextEncoder().encode(str);
  const buf = await crypto.subtle.digest('SHA-256', data);
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

function truncate(s, n) {
  s = String(s || '');
  return s.length > n ? s.slice(0, n) + '…' : s;
}

/** tcb_info may arrive as a nested object or as a JSON string; normalize to an object. */
function parseTcbInfo(tcb) {
  if (!tcb) return {};
  if (typeof tcb === 'string') {
    try {
      return JSON.parse(tcb);
    } catch {
      return {};
    }
  }
  return tcb;
}

// ── Verdict banner ───────────────────────────────────────────────────────────

function setVerdict(status, text, sub) {
  const v = el('verdict');
  v.className = 'verdict ' + status;
  el('verdict-text').textContent = text;
  el('verdict-sub').textContent = sub || '';
}

// ── Verification steps ───────────────────────────────────────────────────────

// Outcomes per step that feed the overall verdict. A verifier has no middle
// ground: a check either proves what it claims (PASS) or it does not (FAIL).
const PASS = 'pass';
const FAIL = 'fail';

/**
 * A verification tool must run over an authenticated channel, otherwise a network
 * attacker can rewrite /info, /attestation, or RPC responses and forge the result.
 * Require https:// everywhere; allow http:// only for loopback (local dev).
 */
function isAllowedUrl(raw) {
  let u;
  try {
    u = new URL(raw);
  } catch {
    return false;
  }
  if (u.protocol === 'https:') return true;
  if (u.protocol === 'http:') {
    return ['localhost', '127.0.0.1', '[::1]', '::1'].includes(u.hostname);
  }
  return false;
}

// A hostile endpoint must not be able to hang the UI forever or exhaust memory.
const FETCH_TIMEOUT_MS = 15000;
const MAX_RESPONSE_BYTES = 4 * 1024 * 1024; // 4 MiB cap on any single response.

/** Read a response body with a hard byte cap so a lying/absent Content-Length can't OOM the tab. */
async function readCapped(res, controller) {
  const declared = Number(res.headers.get('content-length') || 0);
  if (declared > MAX_RESPONSE_BYTES) {
    controller.abort();
    throw new Error(`Response too large (${declared} bytes; cap ${MAX_RESPONSE_BYTES}).`);
  }
  if (!res.body || !res.body.getReader) {
    const text = await res.text();
    if (text.length > MAX_RESPONSE_BYTES) throw new Error(`Response exceeded ${MAX_RESPONSE_BYTES}-byte cap.`);
    return text;
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let total = 0;
  let text = '';
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.length;
    if (total > MAX_RESPONSE_BYTES) {
      controller.abort();
      throw new Error(`Response exceeded ${MAX_RESPONSE_BYTES}-byte cap.`);
    }
    text += decoder.decode(value, { stream: true });
  }
  return text + decoder.decode();
}

async function fetchJson(url) {
  // Always read fresh: a verification tool must never trust a stale cache.
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), FETCH_TIMEOUT_MS);
  let body;
  try {
    const res = await fetch(url, {
      cache: 'no-store',
      signal: controller.signal,
      headers: { Accept: 'application/json', 'Cache-Control': 'no-cache' },
    });
    body = await readCapped(res, controller);
    if (!res.ok) {
      throw new Error(`HTTP ${res.status} — ${truncate(body, 200) || res.statusText}`);
    }
  } catch (e) {
    if (e.name === 'AbortError') {
      throw new Error(`Request to ${url} timed out or was aborted after ${FETCH_TIMEOUT_MS / 1000}s.`);
    }
    throw e;
  } finally {
    clearTimeout(timer);
  }
  try {
    return JSON.parse(body);
  } catch {
    throw new Error(`Response was not valid JSON: ${truncate(body, 200)}`);
  }
}

/** Step 1 — read the live attestation. Returns { info, tcb, quote } or throws. */
async function stepInfo(apiUrl) {
  setPill('step-info', 'running', 'checking');
  setDetail('step-info', '');

  let info;
  try {
    info = await fetchJson(`${apiUrl}/info`);
  } catch (e) {
    setPill('step-info', 'fail', 'failed');
    setDetail(
      'step-info',
      `<p class="note bad">Could not reach <code>${escapeHtml(apiUrl)}/info</code>: ${escapeHtml(e.message)}</p>`
    );
    throw e;
  }

  const tcb = parseTcbInfo(info.tcb_info);
  const composeHash = info.compose_hash || tcb.compose_hash || '';

  // The attestation quote is the artifact that ties this server to real TDX
  // hardware. A genuine TEE always serves one; its absence is disqualifying,
  // even though full DCAP validation of it happens at the Trust Center (step 5).
  let quote = null;
  let quoteNote = '';
  try {
    quote = await fetchJson(`${apiUrl}/attestation`);
  } catch (e) {
    quoteNote = `<p class="note bad">Quote endpoint unavailable: ${escapeHtml(e.message)}</p>`;
  }

  let detail =
    kv('App name', info.app_name) +
    kv('App id', info.app_id) +
    kv('Instance id', info.instance_id) +
    kv('Device id', info.device_id) +
    kv('Compose hash', composeHash) +
    kv('OS image hash', info.os_image_hash || tcb.os_image_hash) +
    kv('Key provider', info.key_provider_info);

  if (quote && quote.quote) {
    detail +=
      kv('TDX quote', `${quote.quote.length} chars · ${truncate(quote.quote, 48)}`) +
      collapsible('Show raw quote', quote.quote);
    if (quote.event_log) detail += collapsible('Show event log', quote.event_log);
  }
  detail += quoteNote;

  if (tcb.app_compose) {
    detail += collapsible('Show app_compose', tcb.app_compose);
  }

  let outcome = PASS;
  if (!composeHash) {
    setPill('step-info', 'fail', 'no compose_hash');
    detail += `<p class="note bad">Server did not report a compose_hash, so its configuration cannot be verified.</p>`;
    outcome = FAIL;
  } else if (!(quote && quote.quote)) {
    setPill('step-info', 'fail', 'no attestation quote');
    detail += `<p class="note bad">Server did not return a TDX attestation quote. A genuine TEE always serves one; without it this endpoint cannot be attested.</p>`;
    outcome = FAIL;
  } else {
    setPill('step-info', 'pass', 'reachable');
  }
  setDetail('step-info', detail);

  return { info, tcb, quote, composeHash, outcome };
}

/** Step 2 — recompute SHA-256(app_compose) and compare to the reported hash. */
async function stepComposeIntegrity(tcb, composeHash) {
  setPill('step-compose', 'running', 'hashing');

  if (!tcb.app_compose) {
    setPill('step-compose', 'fail', 'no app_compose');
    setDetail(
      'step-compose',
      `<p class="note bad">The server did not return <code>app_compose</code>, so its configuration hash cannot be independently recomputed. The on-chain check only proves the server-reported hash is whitelisted, not that the server runs it — integrity is unverifiable.</p>`
    );
    return FAIL;
  }

  let computed;
  try {
    computed = await sha256hex(tcb.app_compose);
  } catch (e) {
    setPill('step-compose', 'fail', 'unavailable');
    setDetail(
      'step-compose',
      `<p class="note bad">Cannot recompute the hash in this browser, so integrity cannot be verified: ${escapeHtml(e.message)}</p>`
    );
    return FAIL;
  }
  const reported = normHex(composeHash);
  const match = computed === reported;

  const detail =
    kv('Reported compose_hash', reported || '—', match ? 'good' : 'bad') +
    kv('SHA-256(app_compose)', computed, match ? 'good' : 'bad') +
    (match
      ? `<p class="note">The configuration the server reports hashes to exactly the value it claims.</p>`
      : `<p class="note bad">Recomputed hash does NOT match the reported compose_hash. The server is misreporting its configuration — do not trust this endpoint.</p>`);
  setDetail('step-compose', detail);

  if (match) {
    setPill('step-compose', 'pass', 'matches');
    return PASS;
  }
  setPill('step-compose', 'fail', 'mismatch');
  return FAIL;
}

/** Step 3 — app_id matches the expected DstackApp address. */
function stepIdentity(info, expectedAppId) {
  setPill('step-identity', 'running', 'checking');

  const actual = normHex(info.app_id);
  const expected = normHex(expectedAppId);
  const match = actual && actual === expected;

  setDetail(
    'step-identity',
    kv('Server app_id', '0x' + (actual || ''), match ? 'good' : 'bad') +
      kv('Expected app_id', '0x' + expected, match ? 'good' : 'bad') +
      (match
        ? `<p class="note">The server identifies as the expected Lit Chipotle deployment, governed by the DstackApp contract at this address.</p>`
        : `<p class="note bad">The server's app_id does not match the expected DstackApp address. You may be talking to a different deployment.</p>`)
  );

  if (match) {
    setPill('step-identity', 'pass', 'matches');
    return PASS;
  }
  setPill('step-identity', 'fail', 'mismatch');
  return FAIL;
}

/** Step 4 — query allowedComposeHashes(bytes32) on Base. */
async function stepOnChain(rpcUrl, dstackAppAddress, composeHash) {
  setPill('step-onchain', 'running', 'querying');

  const stripped = normHex(composeHash);
  if (stripped.length !== 64) {
    setPill('step-onchain', 'fail', 'bad hash');
    setDetail(
      'step-onchain',
      `<p class="note bad">compose_hash is not a 32-byte value (got ${stripped.length} hex chars); cannot query the contract.</p>`
    );
    return FAIL;
  }
  const composeHash0x = '0x' + stripped;

  let allowed;
  try {
    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const contract = new ethers.Contract(dstackAppAddress, DSTACK_APP_ABI, provider);
    allowed = await contract.allowedComposeHashes(composeHash0x);
  } catch (e) {
    setPill('step-onchain', 'fail', 'rpc error');
    setDetail(
      'step-onchain',
      kv('DstackApp', dstackAppAddress) +
        kv('compose_hash', composeHash0x) +
        `<p class="note bad">RPC call failed: ${escapeHtml(e.message || String(e))}</p>`
    );
    return FAIL;
  }

  const detail =
    kv('DstackApp', dstackAppAddress) +
    kv('RPC', rpcUrl) +
    kv('compose_hash', composeHash0x) +
    kv('allowedComposeHashes()', String(allowed), allowed ? 'good' : 'bad') +
    `<p class="note">${link('https://basescan.org/address/' + dstackAppAddress, 'View DstackApp on BaseScan')}</p>` +
    (allowed
      ? `<p class="note">This configuration is whitelisted on Base by the Lit Safe multisig.</p>`
      : `<p class="note bad">This configuration is NOT whitelisted. The CVM would be rejected with "Compose hash not allowed".</p>`);
  setDetail('step-onchain', detail);

  if (allowed) {
    setPill('step-onchain', 'pass', 'whitelisted');
    return PASS;
  }
  setPill('step-onchain', 'fail', 'not whitelisted');
  return FAIL;
}

/** Step 5 — defer the heavy checks to the Phala Trust Center. */
function stepTrustCenter(info, expectedAppId) {
  const appId = normHex(info && info.app_id) || normHex(expectedAppId);
  const trustUrl = `https://trust.phala.com/app/${appId}`;
  setPill('step-trust', 'info', 'manual');
  setDetail(
    'step-trust',
    `<p class="note">${link(trustUrl, 'Open the Phala Trust Center report →')}</p>` +
      `<p class="note">It validates the Intel TDX hardware quote, the OS measurements, and that HTTPS is terminated inside the enclave — automatically, with no install.</p>` +
      `<p class="note">On-chain governance (Base): ${link(
        'https://basescan.org/address/' + expectedAppId,
        'DstackApp'
      )} · ${link('https://basescan.org/address/' + CONTRACTS.kms, 'Phala KMS')} · ${link(
        'https://basescan.org/address/' + CONTRACTS.safe,
        'Safe multisig'
      )}</p>`
  );
}

// ── Orchestration ────────────────────────────────────────────────────────────

let running = false;

/**
 * Read and validate the config inputs up-front. Returns { config } on success or
 * { error } with a message. Fails fast so we never issue a relative fetch (e.g. an
 * empty API URL becoming `/info`) or pass a non-canonical address to ethers.
 */
function readConfig() {
  const apiUrl = el('api-url').value.trim().replace(/\/+$/, '');
  const rpcUrl = el('rpc-url').value.trim();
  const appIdRaw = el('app-id').value.trim();

  if (!isAllowedUrl(apiUrl)) {
    return { error: 'API base URL must be an https:// URL (http:// is allowed only for localhost).' };
  }
  if (!isAllowedUrl(rpcUrl)) {
    return { error: 'Base RPC URL must be an https:// URL (http:// is allowed only for localhost).' };
  }

  // Accept a bare 20-byte hex (no 0x) too, then canonicalize via ethers.getAddress,
  // which validates length and checksum. Step 3 compares it, step 4 calls it.
  let addrInput = appIdRaw;
  if (/^[0-9a-fA-F]{40}$/.test(addrInput)) addrInput = '0x' + addrInput;
  let expectedAppId;
  try {
    expectedAppId = ethers.getAddress(addrInput);
  } catch {
    return { error: 'Expected app id must be a valid 20-byte address (0x + 40 hex chars).' };
  }

  return { config: { apiUrl, rpcUrl, expectedAppId } };
}

async function runVerify() {
  if (running) return;

  // Fail closed if the ethers dependency never loaded (CDN blocked, offline, or
  // SRI integrity mismatch). Without it steps 3 and 4 cannot run.
  if (typeof ethers === 'undefined') {
    setVerdict(
      'fail',
      'Verifier failed to load',
      'The ethers.js library could not be loaded (CDN blocked, offline, or integrity mismatch). Reload the page or check your network and extensions.'
    );
    return;
  }

  const { config, error } = readConfig();
  if (error) {
    setVerdict('fail', 'Invalid configuration', error);
    return;
  }
  const { apiUrl, rpcUrl, expectedAppId } = config;

  running = true;
  el('verify-btn').disabled = true;
  setVerdict('running', 'Verifying…', 'Reading attestation, recomputing the hash, and querying Base.');

  // Reset pills/details.
  for (const id of ['step-info', 'step-compose', 'step-identity', 'step-onchain', 'step-trust']) {
    setPill(id, '', 'pending');
    setDetail(id, '');
  }

  const outcomes = [];
  try {
    const { info, tcb, composeHash, outcome: infoOutcome } = await stepInfo(apiUrl);
    outcomes.push(infoOutcome);

    outcomes.push(await stepComposeIntegrity(tcb, composeHash));
    outcomes.push(stepIdentity(info, expectedAppId));
    outcomes.push(await stepOnChain(rpcUrl, expectedAppId, composeHash));
    stepTrustCenter(info, expectedAppId);

    if (outcomes.includes(FAIL)) {
      setVerdict(
        'fail',
        'Verification failed',
        'One or more checks did not pass. Do not trust this endpoint until resolved.'
      );
    } else {
      setVerdict(
        'pass',
        'Configuration checks passed',
        'The server is reachable, serves an attestation quote, its app_compose hashes to the reported compose_hash, its identity matches, and that hash is whitelisted on Base. This does NOT by itself prove the hardware: complete the Intel TDX quote validation at the Phala Trust Center (step 5) to confirm a genuine enclave.'
      );
    }
  } catch (e) {
    // stepInfo already rendered its own failure detail.
    setVerdict('fail', 'Verification failed', e.message || String(e));
  } finally {
    el('verify-btn').disabled = false;
    running = false;
  }
}

function applyDefaults() {
  el('api-url').value = DEFAULTS.apiUrl;
  el('rpc-url').value = DEFAULTS.rpcUrl;
  el('app-id').value = DEFAULTS.appId;
}

// ── Theme ────────────────────────────────────────────────────────────────────

const THEME_KEY = 'verify-theme';

function applyTheme(theme) {
  document.documentElement.setAttribute('data-theme', theme);
  // The button shows the theme it switches to.
  const switchesToLight = theme === 'dark';
  el('theme-icon').textContent = switchesToLight ? '☀' : '☾';
  el('theme-label').textContent = switchesToLight ? 'Light' : 'Dark';
}

function initTheme() {
  let theme = null;
  try {
    theme = localStorage.getItem(THEME_KEY);
  } catch {
    /* localStorage unavailable */
  }
  if (theme !== 'light' && theme !== 'dark') {
    theme =
      window.matchMedia && window.matchMedia('(prefers-color-scheme: light)').matches
        ? 'light'
        : 'dark';
  }
  applyTheme(theme);
}

function toggleTheme() {
  const next = document.documentElement.getAttribute('data-theme') === 'light' ? 'dark' : 'light';
  applyTheme(next);
  try {
    localStorage.setItem(THEME_KEY, next);
  } catch {
    /* localStorage unavailable */
  }
}

// ── Boot ─────────────────────────────────────────────────────────────────────

initTheme();
applyDefaults();
el('theme-toggle').addEventListener('click', toggleTheme);
el('verify-btn').addEventListener('click', runVerify);
el('reset-btn').addEventListener('click', applyDefaults);
