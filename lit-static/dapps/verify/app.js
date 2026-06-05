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
  return `<a href="${escapeHtml(href)}" target="_blank" rel="noopener">${escapeHtml(text)}</a>`;
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

// Outcomes per step that feed the overall verdict.
const PASS = 'pass';
const WARN = 'warn';
const FAIL = 'fail';

async function fetchJson(url) {
  const res = await fetch(url, { headers: { Accept: 'application/json' } });
  const body = await res.text();
  if (!res.ok) {
    throw new Error(`HTTP ${res.status} — ${truncate(body, 200) || res.statusText}`);
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

  // Attestation quote is supporting evidence; failure here is not fatal.
  let quote = null;
  let quoteNote = '';
  try {
    quote = await fetchJson(`${apiUrl}/attestation`);
  } catch (e) {
    quoteNote = `<p class="note">Quote endpoint unavailable: ${escapeHtml(e.message)}</p>`;
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

  if (composeHash) {
    setPill('step-info', 'pass', 'reachable');
  } else {
    setPill('step-info', 'warn', 'no compose_hash');
    detail += `<p class="note bad">Server did not report a compose_hash.</p>`;
  }
  setDetail('step-info', detail);

  return { info, tcb, quote, composeHash, outcome: composeHash ? PASS : WARN };
}

/** Step 2 — recompute SHA-256(app_compose) and compare to the reported hash. */
async function stepComposeIntegrity(tcb, composeHash) {
  setPill('step-compose', 'running', 'hashing');

  if (!tcb.app_compose) {
    setPill('step-compose', 'warn', 'no app_compose');
    setDetail(
      'step-compose',
      `<p class="note">The server did not return <code>app_compose</code>, so the hash could not be recomputed in-browser. The on-chain whitelist check below remains the authoritative binding.</p>`
    );
    return WARN;
  }

  const computed = await sha256hex(tcb.app_compose);
  const reported = normHex(composeHash);
  const match = computed === reported;

  const detail =
    kv('Reported compose_hash', reported || '—', match ? 'good' : 'bad') +
    kv('SHA-256(app_compose)', computed, match ? 'good' : 'bad') +
    (match
      ? `<p class="note">The configuration the server reports hashes to exactly the value it claims.</p>`
      : `<p class="note bad">Recomputed hash does not match the reported compose_hash. This can happen if the dstack manifest serializes app_compose differently than the hashed bytes; the authoritative check is the on-chain whitelist (step 4), which binds the quoted compose_hash.</p>`);
  setDetail('step-compose', detail);

  if (match) {
    setPill('step-compose', 'pass', 'matches');
    return PASS;
  }
  setPill('step-compose', 'warn', 'mismatch');
  return WARN;
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

async function runVerify() {
  if (running) return;
  running = true;

  const apiUrl = el('api-url').value.trim().replace(/\/+$/, '');
  const rpcUrl = el('rpc-url').value.trim();
  const expectedAppId = el('app-id').value.trim();

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
    } else if (outcomes.includes(WARN)) {
      setVerdict(
        'warn',
        'Verified with warnings',
        'Core checks passed. Review the warnings below, then confirm the hardware quote via the Phala Trust Center.'
      );
    } else {
      setVerdict(
        'pass',
        'Verified',
        'Reachable, configuration hash matches, identity confirmed, and whitelisted on Base. Confirm the hardware quote via the Phala Trust Center (step 5).'
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
