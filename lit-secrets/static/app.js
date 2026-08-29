// Lit Secrets dashboard. Vanilla JS, no build step.

const $ = (sel) => document.querySelector(sel);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const fmt = (iso) => (iso ? new Date(iso).toLocaleString() : '—');

async function api(method, path, body) {
  const res = await fetch(path, {
    method,
    headers: body ? { 'Content-Type': 'application/json' } : {},
    body: body ? JSON.stringify(body) : undefined,
    credentials: 'same-origin',
  });
  if (res.status === 401) {
    window.location.href = '/login';
    throw new Error('unauthorized');
  }
  const text = await res.text();
  let data = null;
  try { data = text ? JSON.parse(text) : null; } catch { data = { raw: text }; }
  if (!res.ok) {
    const msg = data && data.error ? `${data.error}${data.detail ? ': ' + data.detail : ''}` : `HTTP ${res.status}`;
    throw new Error(msg);
  }
  return data;
}

// ---------- vault ----------
async function loadVault() {
  const t = await api('GET', '/api/tenant');
  $('#vault-status').textContent = t.provisioned ? 'provisioned' : 'not provisioned (created on first secret)';
  $('#vault-status').className = 'pill ' + (t.provisioned ? 'ok' : '');
  $('#vault-kv').innerHTML = [
    ['Vault PKP', t.pkp_id || '—'],
    ['Chipotle group', t.group_id ?? '—'],
    ['Reader action CID', t.reader_cid],
    ['Encrypt action CID', t.encrypt_cid],
    ['Grant signer', t.grant_signer],
    ['Chipotle', t.chipotle_api_base_url],
  ].map(([k, v]) => `<dt>${esc(k)}</dt><dd><code>${esc(v)}</code></dd>`).join('');
  $('#vault-stale').classList.toggle('hidden', !t.reader_cid_stale);
}

// ---------- secrets ----------
function policySummary(p) {
  const parts = [];
  if (p.allowed_agents) parts.push(`${p.allowed_agents.length} agent(s)`);
  else parts.push('all agents');
  if (p.max_reads_per_day) parts.push(`≤${p.max_reads_per_day}/day`);
  if (p.not_after) parts.push(`until ${fmt(p.not_after)}`);
  return parts.join(' · ');
}

async function loadSecrets() {
  const list = await api('GET', '/api/secrets');
  const el = $('#secrets');
  if (!list.length) { el.innerHTML = '<p class="muted">No secrets yet.</p>'; return; }
  el.innerHTML = list.map((s) => `
    <div class="trigger-card ${s.disabled ? 'disabled' : ''}" data-name="${esc(s.name)}">
      <div class="trigger-title">
        <strong><code>${esc(s.name)}</code></strong>
        <span class="pill ${s.release === 'in_tee_only' ? 'warn' : 'ok'}">${esc(s.release)}</span>
      </div>
      <div class="muted">v${s.current_version} · ${esc(s.kind)} · ${esc(s.environment)} · ${esc(policySummary(s.policy))}${s.disabled ? ' · <b>disabled</b>' : ''}</div>
      <div class="row actions">
        <button data-act="rotate">Rotate</button>
        <button data-act="release">${s.release === 'plaintext' ? 'Make in-TEE-only' : 'Make plaintext'}</button>
        <button data-act="toggle">${s.disabled ? 'Enable' : 'Disable'}</button>
        <button data-act="agents">Restrict agents…</button>
        <button data-act="delete" class="danger">Delete</button>
      </div>
    </div>`).join('');
}

$('#secrets').addEventListener('click', async (e) => {
  const btn = e.target.closest('button[data-act]');
  if (!btn) return;
  const name = btn.closest('.trigger-card').dataset.name;
  const act = btn.dataset.act;
  try {
    if (act === 'rotate') {
      const value = prompt(`New value for ${name}:`);
      if (!value) return;
      await api('PUT', `/api/secrets/${encodeURIComponent(name)}`, { value });
    } else if (act === 'release') {
      const cur = btn.textContent.includes('in-TEE') ? 'in_tee_only' : 'plaintext';
      await api('PATCH', `/api/secrets/${encodeURIComponent(name)}`, { release: cur });
    } else if (act === 'toggle') {
      await api('PATCH', `/api/secrets/${encodeURIComponent(name)}`, { disabled: btn.textContent === 'Disable' });
    } else if (act === 'agents') {
      const agents = await api('GET', '/api/agents');
      const live = agents.filter((a) => !a.revoked_at);
      const pick = prompt(`Comma-separated agent names allowed to read ${name} (blank = all):\n${live.map((a) => a.name).join(', ')}`);
      if (pick === null) return;
      const names = pick.split(',').map((s) => s.trim()).filter(Boolean);
      const ids = live.filter((a) => names.includes(a.name)).map((a) => a.id);
      const detail = await api('GET', `/api/secrets/${encodeURIComponent(name)}`);
      const policy = { ...detail.policy };
      if (ids.length) policy.allowed_agents = ids; else delete policy.allowed_agents;
      await api('PATCH', `/api/secrets/${encodeURIComponent(name)}`, { policy });
    } else if (act === 'delete') {
      if (!confirm(`Delete ${name} and all its versions?`)) return;
      await api('DELETE', `/api/secrets/${encodeURIComponent(name)}`);
    }
    await Promise.all([loadSecrets(), loadAudit()]);
  } catch (err) { alert(err.message); }
});

$('#new-secret-btn').addEventListener('click', () => $('#secret-form').classList.toggle('hidden'));
$('#secret-cancel').addEventListener('click', () => $('#secret-form').classList.add('hidden'));
$('#secret-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = new FormData(e.target);
  const policy = {};
  if (f.get('max_reads_per_day')) policy.max_reads_per_day = Number(f.get('max_reads_per_day'));
  if (f.get('not_after')) policy.not_after = new Date(f.get('not_after')).toISOString();
  $('#secret-error').textContent = '';
  const submit = e.target.querySelector('button[type=submit]');
  submit.disabled = true; submit.textContent = 'Sealing in TEE…';
  try {
    await api('POST', '/api/secrets', {
      name: f.get('name'), value: f.get('value'), kind: f.get('kind') || undefined,
      environment: f.get('environment') || undefined, release: f.get('release'), policy,
    });
    e.target.reset();
    $('#secret-form').classList.add('hidden');
    await Promise.all([loadSecrets(), loadVault()]);
  } catch (err) {
    $('#secret-error').textContent = err.message;
  } finally {
    submit.disabled = false; submit.textContent = 'Seal & save';
  }
});

// ---------- agents ----------
async function loadAgents() {
  const list = await api('GET', '/api/agents');
  const el = $('#agents');
  if (!list.length) { el.innerHTML = '<p class="muted">No agents yet.</p>'; return; }
  el.innerHTML = list.map((a) => `
    <div class="trigger-card ${a.revoked_at ? 'disabled' : ''}" data-id="${esc(a.id)}">
      <div class="trigger-title">
        <strong>${esc(a.name)}</strong>
        <span class="pill ${a.revoked_at ? '' : 'ok'}">${a.revoked_at ? 'revoked' : 'active'}</span>
      </div>
      <div class="muted"><code>${esc(a.id)}</code> · created ${fmt(a.created_at)} · last seen ${fmt(a.last_seen_at)}</div>
      ${a.revoked_at ? '' : '<div class="row actions"><button data-act="revoke" class="danger">Revoke</button></div>'}
    </div>`).join('');
}

$('#agents').addEventListener('click', async (e) => {
  const btn = e.target.closest('button[data-act=revoke]');
  if (!btn) return;
  const id = btn.closest('.trigger-card').dataset.id;
  if (!confirm('Revoke this agent key? It stops working on Chipotle immediately.')) return;
  try { await api('DELETE', `/api/agents/${id}`); await loadAgents(); } catch (err) { alert(err.message); }
});

$('#agent-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = new FormData(e.target);
  const btn = e.target.querySelector('button');
  btn.disabled = true;
  try {
    const a = await api('POST', '/api/agents', { name: f.get('name') });
    const box = $('#agent-new');
    box.classList.remove('hidden');
    box.innerHTML = `<b>Key for ${esc(a.name)} — copy it now, it will not be shown again:</b><pre><code>${esc(a.usage_api_key)}</code></pre>
      <div class="muted">Set as <code>LIT_SECRETS_KEY</code> for the agent. Chipotle: <code>${esc(a.chipotle_api_base_url)}</code></div>`;
    e.target.reset();
    await Promise.all([loadAgents(), loadVault()]);
  } catch (err) { alert(err.message); } finally { btn.disabled = false; }
});

// ---------- tenant actions ----------
async function loadActions() {
  const list = await api('GET', '/api/actions');
  const el = $('#actions');
  if (!list.length) { el.innerHTML = '<p class="muted">No customer actions attached.</p>'; return; }
  el.innerHTML = list.map((a) => `
    <div class="trigger-card" data-id="${esc(a.id)}">
      <div class="trigger-title"><strong>${esc(a.name)}</strong><button data-act="detach" class="danger">Detach</button></div>
      <div class="muted"><code>${esc(a.cid)}</code> · ${fmt(a.created_at)}</div>
    </div>`).join('');
}

$('#actions').addEventListener('click', async (e) => {
  const btn = e.target.closest('button[data-act=detach]');
  if (!btn) return;
  const id = btn.closest('.trigger-card').dataset.id;
  try { await api('DELETE', `/api/actions/${id}`); await loadActions(); } catch (err) { alert(err.message); }
});

$('#action-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = new FormData(e.target);
  try {
    await api('POST', '/api/actions', { cid: f.get('cid'), name: f.get('name') || undefined });
    e.target.reset();
    await Promise.all([loadActions(), loadVault()]);
  } catch (err) { alert(err.message); }
});

// ---------- audit ----------
async function loadAudit() {
  const rows = await api('GET', '/api/audit?limit=100');
  $('#audit tbody').innerHTML = rows.length
    ? rows.map((r) => `<tr class="${r.decision}">
        <td>${esc(fmt(r.created_at))}</td><td>${esc(r.event)}</td>
        <td><code>${esc(r.secret_name || '—')}</code></td><td>${esc(r.agent_name || '—')}</td>
        <td>${esc(r.decision)}</td><td>${esc(r.reason || '')}</td></tr>`).join('')
    : '<tr><td colspan="6" class="muted">No access yet.</td></tr>';
}
$('#audit-refresh').addEventListener('click', loadAudit);

// ---------- boot ----------
$('#logout').addEventListener('click', async () => {
  await fetch('/auth/logout', { method: 'POST', credentials: 'same-origin' });
  window.location.href = '/login';
});

(async () => {
  document.querySelectorAll('.origin').forEach((el) => { el.textContent = window.location.origin; });
  try {
    const me = await api('GET', '/api/me');
    $('#me').textContent = me.email;
    await Promise.all([loadVault(), loadSecrets(), loadAgents(), loadActions(), loadAudit()]);
  } catch (err) {
    console.error(err);
  }
})();
