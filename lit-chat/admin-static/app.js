// Lit Chat Admin console. Vanilla ES module, no CDN, no innerHTML with
// server data (all rendering via createElement/textContent).

const $ = (id) => document.getElementById(id);

let csrf = null;

async function req(method, path, body) {
  const headers = {};
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (method !== 'GET' && csrf) headers['X-CSRF-Token'] = csrf;
  const res = await fetch(path, {
    method,
    headers,
    credentials: 'same-origin',
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) {
    let slug = 'server_error';
    try { slug = (await res.json()).error || slug; } catch { /* not json */ }
    throw Object.assign(new Error(slug), { code: res.status, slug });
  }
  return res.json();
}

// --------------------------------------------------------------------------
// WebAuthn helpers: base64url <-> ArrayBuffer conversions for the
// challenge/credential JSON the server (webauthn-rs) speaks.

function b64uToBuf(s) {
  const pad = '='.repeat((4 - (s.length % 4)) % 4);
  const raw = atob(s.replace(/-/g, '+').replace(/_/g, '/') + pad);
  const buf = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i += 1) buf[i] = raw.charCodeAt(i);
  return buf.buffer;
}

function bufToB64u(buf) {
  const bytes = new Uint8Array(buf);
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

async function registerPasskey() {
  const ccr = await req('POST', '/admin/api/webauthn/register/start');
  const pk = ccr.publicKey;
  pk.challenge = b64uToBuf(pk.challenge);
  pk.user.id = b64uToBuf(pk.user.id);
  if (pk.excludeCredentials) {
    pk.excludeCredentials = pk.excludeCredentials.map((c) => ({ ...c, id: b64uToBuf(c.id) }));
  }
  const cred = await navigator.credentials.create({ publicKey: pk });
  const payload = {
    id: cred.id,
    rawId: bufToB64u(cred.rawId),
    type: cred.type,
    extensions: cred.getClientExtensionResults(),
    response: {
      attestationObject: bufToB64u(cred.response.attestationObject),
      clientDataJSON: bufToB64u(cred.response.clientDataJSON),
    },
  };
  return req('POST', '/admin/api/webauthn/register/finish', payload);
}

async function authPasskey() {
  const rcr = await req('POST', '/admin/api/webauthn/auth/start');
  const pk = rcr.publicKey;
  pk.challenge = b64uToBuf(pk.challenge);
  if (pk.allowCredentials) {
    pk.allowCredentials = pk.allowCredentials.map((c) => ({ ...c, id: b64uToBuf(c.id) }));
  }
  const cred = await navigator.credentials.get({ publicKey: pk });
  const payload = {
    id: cred.id,
    rawId: bufToB64u(cred.rawId),
    type: cred.type,
    extensions: cred.getClientExtensionResults(),
    response: {
      authenticatorData: bufToB64u(cred.response.authenticatorData),
      clientDataJSON: bufToB64u(cred.response.clientDataJSON),
      signature: bufToB64u(cred.response.signature),
      userHandle: cred.response.userHandle ? bufToB64u(cred.response.userHandle) : null,
    },
  };
  return req('POST', '/admin/api/webauthn/auth/finish', payload);
}

// --------------------------------------------------------------------------
// Login flow

function showError(msg) {
  const el = $('login-error');
  el.textContent = msg;
  el.classList.remove('hidden');
}

let passkeyMode = null; // 'register' | 'auth'

$('btn-request').addEventListener('click', async () => {
  const email = $('email').value.trim();
  if (!email) return;
  try {
    await req('POST', '/admin/api/auth/request', { email });
    $('step-email').classList.add('hidden');
    $('step-code').classList.remove('hidden');
    $('login-error').classList.add('hidden');
  } catch {
    showError('Request failed.');
  }
});

$('btn-verify').addEventListener('click', async () => {
  const code = $('code').value.trim();
  if (!code) return;
  try {
    const res = await req('POST', '/admin/api/auth/verify', { code });
    $('step-code').classList.add('hidden');
    $('step-passkey').classList.remove('hidden');
    if (res.next === 'register_passkey') {
      passkeyMode = 'register';
      $('passkey-msg').textContent =
        'First admin login: register a passkey now. It becomes mandatory for every future login.';
      $('btn-passkey').textContent = 'Register passkey';
    } else {
      passkeyMode = 'auth';
      $('passkey-msg').textContent = 'Confirm with your passkey.';
      $('btn-passkey').textContent = 'Use passkey';
    }
  } catch (e) {
    showError(e.slug === 'not_an_admin' ? 'Not an admin.' : 'Code invalid or expired.');
  }
});

$('btn-passkey').addEventListener('click', async () => {
  try {
    const res = passkeyMode === 'register' ? await registerPasskey() : await authPasskey();
    csrf = res.csrf;
    await enterConsole();
  } catch {
    showError('Passkey ceremony failed.');
  }
});

// --------------------------------------------------------------------------
// Console

async function enterConsole() {
  $('login-view').classList.add('hidden');
  $('console-view').classList.remove('hidden');
  const me = await req('GET', '/admin/api/me');
  csrf = me.csrf;
  $('whoami').textContent = me.user_ref_hash.slice(0, 12) + '…';
  await Promise.all([refreshStatus(), refreshKeys(), refreshModels(), refreshAdmins(), refreshAudit()]);
}

function kvRow(k, v) {
  const row = document.createElement('div');
  const kEl = document.createElement('span');
  kEl.className = 'k';
  kEl.textContent = k;
  const vEl = document.createElement('span');
  vEl.className = 'v mono';
  vEl.textContent = v;
  row.append(kEl, vEl);
  return row;
}

const usd = (micro) => `$${(micro / 1e6).toFixed(4)}`;

async function refreshStatus() {
  const s = await req('GET', '/admin/api/status');
  const body = $('status-body');
  body.replaceChildren();
  body.append(
    kvRow('Breaker', typeof s.breaker_mode === 'string' ? s.breaker_mode : JSON.stringify(s.breaker_mode)),
    kvRow('Spend today', usd(s.spend_today_micro_usd)),
    kvRow('Daily cap', usd(s.caps.daily_spend_cap_micro_usd)),
    kvRow('Anon tokens/day', String(s.caps.anon_daily_token_budget)),
    kvRow('Active key', s.active_key_hint || '— none —'),
  );
  if (s.credits) {
    const remaining = s.credits.total_credits - s.credits.total_usage;
    body.append(kvRow('OpenRouter credits left', `$${remaining.toFixed(2)}`));
    if (remaining < 10) {
      const warn = document.createElement('div');
      warn.className = 'error';
      warn.textContent = 'Low OpenRouter balance — top up soon.';
      body.append(warn);
    }
  }
  for (const d of s.recent_days || []) {
    body.append(kvRow(d.day, `${usd(d.micro_usd)} (${d.requests} req)`));
  }
  $('cap-usd').value = (s.caps.daily_spend_cap_micro_usd / 1e6).toFixed(2);
  $('anon-budget').value = s.caps.anon_daily_token_budget;
  const mode = typeof s.breaker_mode === 'string' ? s.breaker_mode : 'auto';
  $('breaker-select').value = mode;
}

function actionBtn(label, fn, danger = false) {
  const b = document.createElement('button');
  b.className = 'btn btn-small' + (danger ? ' btn-danger' : '');
  b.textContent = label;
  b.addEventListener('click', async () => {
    b.disabled = true;
    try { await fn(); } catch (e) { alert(`Failed: ${e.slug || e.message}`); }
    b.disabled = false;
    await Promise.all([refreshKeys(), refreshStatus(), refreshAudit()]);
  });
  return b;
}

async function refreshKeys() {
  const keys = await req('GET', '/admin/api/keys');
  const tbody = $('keys-table').querySelector('tbody');
  tbody.replaceChildren();
  for (const k of keys) {
    const tr = document.createElement('tr');
    const cells = [
      k.masked_hint,
      k.kind,
      k.status,
      k.spend_limit_usd != null ? `$${k.spend_limit_usd}` : '—',
    ];
    for (const c of cells) {
      const td = document.createElement('td');
      td.textContent = c;
      td.className = 'mono';
      tr.appendChild(td);
    }
    const actions = document.createElement('td');
    if (k.kind === 'runtime' && k.status !== 'disabled') {
      if (k.status !== 'active') {
        actions.appendChild(actionBtn('Promote', () => req('POST', `/admin/api/keys/${k.id}/promote`, {})));
      } else {
        actions.appendChild(actionBtn('Retire', () => req('POST', `/admin/api/keys/${k.id}/retire`, {})));
      }
      actions.appendChild(actionBtn('Probe', async () => {
        const r = await req('POST', `/admin/api/keys/${k.id}/probe`, {});
        alert(r.ok ? `Probe OK (${r.model})` : `Probe FAILED (${r.model})`);
      }));
      actions.appendChild(actionBtn('Disable', () => {
        if (!confirm('Disable this key (and delete it upstream if minted here)?')) return Promise.resolve();
        return req('POST', `/admin/api/keys/${k.id}/disable`, {});
      }, true));
    }
    tr.appendChild(actions);
    tbody.appendChild(tr);
  }
}

$('btn-mint').addEventListener('click', async () => {
  const name = $('mint-name').value.trim();
  if (!name) return;
  const limit = parseFloat($('mint-limit').value);
  try {
    await req('POST', '/admin/api/keys/mint', {
      name,
      spend_limit_usd: Number.isFinite(limit) ? limit : null,
    });
    $('mint-name').value = '';
    $('mint-limit').value = '';
    await Promise.all([refreshKeys(), refreshAudit()]);
  } catch (e) {
    alert(`Mint failed: ${e.slug || e.message}`);
  }
});

$('btn-import').addEventListener('click', async () => {
  const key = $('import-key').value.trim();
  if (!key) return;
  try {
    await req('POST', '/admin/api/keys/import', {
      key,
      kind: $('import-kind').value,
      spend_limit_usd: null,
    });
    $('import-key').value = '';
    await Promise.all([refreshKeys(), refreshAudit()]);
  } catch (e) {
    alert(`Import failed: ${e.slug || e.message}`);
  }
});

$('btn-breaker').addEventListener('click', async () => {
  try {
    await req('POST', '/admin/api/breaker', { mode: $('breaker-select').value });
    await Promise.all([refreshStatus(), refreshAudit()]);
  } catch (e) {
    alert(`Failed: ${e.slug || e.message}`);
  }
});

$('btn-caps').addEventListener('click', async () => {
  const capUsd = parseFloat($('cap-usd').value);
  const anon = parseInt($('anon-budget').value, 10);
  if (!Number.isFinite(capUsd) || !Number.isInteger(anon)) return;
  try {
    await req('POST', '/admin/api/caps', {
      daily_spend_cap_micro_usd: Math.round(capUsd * 1e6),
      anon_daily_token_budget: anon,
    });
    await Promise.all([refreshStatus(), refreshAudit()]);
  } catch (e) {
    alert(`Failed: ${e.slug || e.message}`);
  }
});

async function refreshModels() {
  const models = await req('GET', '/admin/api/models');
  const tbody = $('models-table').querySelector('tbody');
  tbody.replaceChildren();
  for (const m of models) {
    const tr = document.createElement('tr');
    const name = document.createElement('td');
    name.textContent = `${m.display_name} (${m.model_id})`;
    const zdr = document.createElement('td');
    zdr.textContent = m.zdr ? 'yes' : 'NO';
    const en = document.createElement('td');
    const toggle = document.createElement('input');
    toggle.type = 'checkbox';
    toggle.checked = m.enabled;
    toggle.addEventListener('change', async () => {
      try {
        await req('POST', '/admin/api/models/toggle', { model_id: m.model_id, enabled: toggle.checked });
        await refreshAudit();
      } catch (e) {
        toggle.checked = !toggle.checked;
        alert(`Failed: ${e.slug || e.message}`);
      }
    });
    en.appendChild(toggle);
    tr.append(name, zdr, en);
    tbody.appendChild(tr);
  }
}

async function refreshAdmins() {
  const admins = await req('GET', '/admin/api/admins');
  const tbody = $('admins-table').querySelector('tbody');
  tbody.replaceChildren();
  for (const a of admins) {
    const tr = document.createElement('tr');
    const hash = document.createElement('td');
    hash.className = 'mono';
    hash.textContent = a.user_ref_hash.slice(0, 16) + '…';
    const by = document.createElement('td');
    by.className = 'mono';
    by.textContent = a.granted_by === 'bootstrap' ? 'bootstrap' : a.granted_by.slice(0, 12) + '…';
    const mac = document.createElement('td');
    mac.textContent = a.mac_valid ? 'valid' : 'INVALID';
    if (!a.mac_valid) mac.className = 'error';
    const act = document.createElement('td');
    act.appendChild(actionBtn('Revoke', () => {
      if (!confirm('Revoke this admin?')) return Promise.resolve();
      return req('DELETE', `/admin/api/admins/${a.user_ref_hash}`);
    }, true));
    tr.append(hash, by, mac, act);
    tbody.appendChild(tr);
  }
}

$('btn-grant').addEventListener('click', async () => {
  const email = $('grant-email').value.trim();
  if (!email) return;
  try {
    await req('POST', '/admin/api/admins', { email });
    $('grant-email').value = '';
    await Promise.all([refreshAdmins(), refreshAudit()]);
  } catch (e) {
    alert(`Failed: ${e.slug || e.message}`);
  }
});

async function refreshAudit() {
  const rows = await req('GET', '/admin/api/audit?limit=100');
  const tbody = $('audit-table').querySelector('tbody');
  tbody.replaceChildren();
  for (const r of rows) {
    const tr = document.createElement('tr');
    const cells = [
      new Date(r.created_at_unix * 1000).toISOString().replace('T', ' ').slice(0, 19),
      r.actor_ref_hash.slice(0, 10) + '…',
      r.action,
      r.subject.length > 24 ? r.subject.slice(0, 24) + '…' : r.subject,
      r.detail,
      r.mac_valid && r.chain_valid ? 'ok' : 'TAMPERED',
    ];
    for (const [i, c] of cells.entries()) {
      const td = document.createElement('td');
      td.textContent = c;
      td.className = 'mono';
      if (i === 5 && c !== 'ok') td.className = 'error mono';
      tr.appendChild(td);
    }
    tbody.appendChild(tr);
  }
}

$('btn-logout').addEventListener('click', async () => {
  try { await req('POST', '/admin/api/auth/logout', {}); } catch { /* already gone */ }
  location.reload();
});

// Session resume: if a full admin session cookie is present, skip login.
(async () => {
  try {
    const me = await req('GET', '/admin/api/me');
    csrf = me.csrf;
    await enterConsole();
  } catch {
    /* stay on login view */
  }
})();
