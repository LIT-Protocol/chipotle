// Lit Agent Keychain dashboard. Vanilla JS, no build step.

const $ = (sel) => document.querySelector(sel);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const fmt = (iso) => (iso ? new Date(iso).toLocaleString() : '—');
const shortId = (id) => String(id ?? '').slice(-8);

// Server error codes → something a person can act on. Unknown codes fall
// through unchanged so diagnostics are never lost.
const ERROR_TEXT = {
  secret_exists: (d) => `A secret named ${d.name || 'that'} already exists. Rotate it from its card instead of creating a duplicate, or pick a different name.`,
  invalid_name: () => 'Name must be 1–128 characters of letters, digits, "_", "." or "-", starting with a letter or digit. Spaces are not allowed.',
  invalid_value: () => 'Value must not be empty.',
  value_too_large: () => 'Value is too large. The limit is 16 KB.',
  invalid_label: () => 'Kind and environment must be 1–64 characters of letters, digits, "_", "." or "-".',
  invalid_policy: (d, detail) => `Policy rejected: ${detail || 'check the quota and agent selection'}.`,
  agent_name_exists: (d) => `An active agent named ${d.name || 'that'} already exists. Choose a different name, or revoke the old one first.`,
  agent_limit_reached: () => 'You have reached the limit of 100 active agents. Revoke one before minting another.',
  secret_limit_reached: () => 'You have reached the limit of 500 secrets.',
  reader_not_attached: () => 'The reader action for this deployment is not attached to your vault yet. An operator needs to re-attach it.',
  not_found: () => 'Not found. It may have been deleted in another tab — refresh the list.',
};

class ApiError extends Error {
  constructor(status, code, detail) {
    super(code ? `${code}${detail ? ': ' + detail : ''}` : `HTTP ${status}`);
    this.status = status; this.code = code; this.detail = detail;
  }
}

function friendly(err, ctx = {}) {
  if (err instanceof ApiError && ERROR_TEXT[err.code]) return ERROR_TEXT[err.code](ctx, err.detail);
  if (err instanceof ApiError && err.status === 502) return `Chipotle (the TEE network) returned an error: ${err.detail || err.code}. This is usually transient — try again.`;
  if (err instanceof TypeError) return 'Network error — check your connection and try again.';
  return err.message;
}

async function api(method, path, body) {
  const res = await fetch(path, {
    method,
    headers: body ? { 'Content-Type': 'application/json' } : {},
    body: body ? JSON.stringify(body) : undefined,
    credentials: 'same-origin',
  });
  if (res.status === 401) {
    window.location.href = '/login';
    throw new ApiError(401, 'unauthorized');
  }
  const text = await res.text();
  let data = null;
  try { data = text ? JSON.parse(text) : null; } catch { data = { raw: text }; }
  if (!res.ok) throw new ApiError(res.status, data && data.error, data && data.detail);
  return data;
}

// ---------- section loading with visible failure + retry (KC-08) ----------
// Every section renders one of: loading / loaded / failed. A failed fetch must
// never look like "no data".
const loaders = {};
function failureBlock(section, err) {
  return `<div class="load-error" role="alert">Couldn't load ${esc(section)}: ${esc(friendly(err))}
    <button class="ghost compact" data-retry="${esc(section)}">Retry</button></div>`;
}
async function loadSection(section) {
  const host = document.querySelector(`[data-section="${section}"]`);
  const loader = loaders[section];
  if (!host || !loader) return;
  if (!host.dataset.loaded) host.innerHTML = '<p class="muted">Loading…</p>';
  try {
    await loader();
    host.dataset.loaded = '1';
  } catch (err) {
    if (err.code === 'unauthorized') return;
    console.error(`load ${section}`, err);
    host.innerHTML = failureBlock(section, err);
  }
}
document.addEventListener('click', (e) => {
  const btn = e.target.closest('button[data-retry]');
  if (btn) loadSection(btn.dataset.retry);
});
const reload = (...sections) => Promise.all(sections.map(loadSection));

// ---------- vault ----------
loaders.vault = async () => {
  const t = await api('GET', '/api/tenant');
  $('#vault-status').textContent = t.provisioned ? 'provisioned' : 'not provisioned yet — created on first secret';
  $('#vault-status').className = 'pill ' + (t.provisioned ? 'ok' : '');
  $('#vault-kv').innerHTML = [
    ['Vault key (PKP)', t.pkp_id || '—'],
    ['Chipotle group', t.group_id ?? '—'],
    ['Reader action CID', t.reader_cid],
    ['Encrypt action CID', t.encrypt_cid],
    ['Grant signer', t.grant_signer],
    ['Chipotle', t.chipotle_api_base_url],
  ].map(([k, v]) => `<dt>${esc(k)}</dt><dd><code>${esc(v)}</code></dd>`).join('');
  $('#vault-stale').classList.toggle('hidden', !t.reader_cid_stale);
  $('#vault-state').innerHTML = '';
};

// ---------- secrets ----------
let agentCache = [];
const agentName = (id) => {
  const a = agentCache.find((x) => x.id === id);
  return a ? `${a.name}${a.revoked_at ? ' (revoked)' : ''}` : `…${shortId(id)}`;
};

function policySummary(p) {
  const parts = [];
  if (p.allowed_agents) parts.push(`${p.allowed_agents.length} agent(s): ${p.allowed_agents.map(agentName).join(', ')}`);
  else parts.push('all agents');
  if (p.max_reads_per_day) parts.push(`≤${p.max_reads_per_day} reads / rolling 24 h`);
  if (p.not_after) parts.push(`until ${fmt(p.not_after)}`);
  return parts.join(' · ');
}

loaders.secrets = async () => {
  const [list, agents] = await Promise.all([api('GET', '/api/secrets'), api('GET', '/api/agents')]);
  agentCache = agents;
  const el = $('#secrets');
  const first = list[0];
  document.querySelectorAll('.example-secret').forEach((n) => { n.textContent = first ? first.name : 'OPENAI_API_KEY'; });
  if (!list.length) { el.innerHTML = '<p class="muted">No secrets yet. Use <b>+ Add secret</b> above.</p>'; return; }
  el.innerHTML = list.map((s) => `
    <div class="trigger-card ${s.disabled ? 'disabled' : ''}" data-name="${esc(s.name)}" data-release="${esc(s.release)}" data-disabled="${s.disabled ? 1 : 0}">
      <div class="trigger-title">
        <strong><code>${esc(s.name)}</code></strong>
        <span class="pill ${s.release === 'in_tee_only' ? 'warn' : 'ok'}">${s.release === 'in_tee_only' ? 'in-TEE-only' : 'plaintext'}</span>
      </div>
      <div class="muted">v${s.current_version} · ${esc(s.kind)} · ${esc(s.environment)}${s.disabled ? ' · <b>disabled</b>' : ''}</div>
      <div class="muted">Policy: ${esc(policySummary(s.policy))}</div>
      <div class="row actions">
        <button class="compact" data-act="policy">Edit policy…</button>
        <button class="compact" data-act="rotate">Rotate…</button>
        <button class="compact" data-act="toggle">${s.disabled ? 'Enable' : 'Disable'}</button>
        <button class="compact ${s.release === 'plaintext' ? '' : 'warnbtn'}" data-act="release">${s.release === 'plaintext' ? 'Make in-TEE-only' : 'Make plaintext…'}</button>
        <button class="compact danger" data-act="delete">Delete…</button>
      </div>
      <details class="versions" data-name="${esc(s.name)}">
        <summary class="muted">Versions</summary>
        <div class="versions-body muted">Loading…</div>
      </details>
    </div>`).join('');
};

$('#secrets').addEventListener('toggle', async (e) => {
  const d = e.target;
  if (!(d instanceof HTMLDetailsElement) || !d.classList.contains('versions') || !d.open) return;
  const body = d.querySelector('.versions-body');
  try {
    const detail = await api('GET', `/api/secrets/${encodeURIComponent(d.dataset.name)}`);
    body.innerHTML = `<table class="table"><thead><tr><th>Version</th><th>Sealed</th><th>Ciphertext hash</th></tr></thead><tbody>${
      detail.versions.map((v) => `<tr><td>v${v.version}${v.version === detail.current_version ? ' <span class="pill ok">current</span>' : ''}</td><td>${esc(fmt(v.created_at))}</td><td><code>${esc(v.ciphertext_hash.slice(0, 18))}…</code></td></tr>`).join('')
    }</tbody></table>
    <p class="hint">Agents read the current version by default. To read an older one, pass its number: <code>keychain.get('${esc(detail.name)}', { version: N })</code>. Rotation does not invalidate older versions; disable or delete the secret to stop all reads.</p>`;
  } catch (err) { body.innerHTML = `<div class="load-error">${esc(friendly(err))}</div>`; }
}, true);

// --- policy editor dialog (KC-03, KC-06) ---
const policyDialog = $('#policy-dialog');
const toLocalInput = (iso) => {
  if (!iso) return '';
  const d = new Date(iso);
  const pad = (n) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
};
async function openPolicyEditor(name) {
  const [detail, agents] = await Promise.all([api('GET', `/api/secrets/${encodeURIComponent(name)}`), api('GET', '/api/agents')]);
  agentCache = agents;
  const p = detail.policy || {};
  const allowed = new Set(p.allowed_agents || []);
  const f = $('#policy-form');
  $('#policy-secret-name').textContent = name;
  f.dataset.name = name;
  f.elements.scope.value = p.allowed_agents ? 'some' : 'all';
  f.elements.max_reads_per_day.value = p.max_reads_per_day || '';
  f.elements.not_after.value = toLocalInput(p.not_after);
  $('#policy-error').textContent = '';
  // Selection is keyed by immutable agent id; the name is only a label, and
  // the id suffix + created time disambiguate agents that share a name.
  const live = agents.filter((a) => !a.revoked_at || allowed.has(a.id));
  $('#policy-agents').innerHTML = live.length
    ? live.map((a) => `<label class="checkbox-row">
        <input type="checkbox" name="agent" value="${esc(a.id)}" ${allowed.has(a.id) ? 'checked' : ''} />
        <span>${esc(a.name)}${a.revoked_at ? ' <span class="pill">revoked</span>' : ''}
          <span class="muted"> · id …${esc(shortId(a.id))} · minted ${esc(fmt(a.created_at))}</span></span>
      </label>`).join('')
    : '<p class="muted">No agents minted yet — every agent you mint later will be able to read this secret while "All agents" is selected.</p>';
  policyDialog.showModal();
}
$('#policy-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = e.target;
  const name = f.dataset.name;
  const policy = {};
  if (f.elements.scope.value === 'some') {
    const ids = [...f.querySelectorAll('input[name=agent]:checked')].map((i) => i.value);
    if (!ids.length) { $('#policy-error').textContent = 'Select at least one agent, or choose "All agents".'; return; }
    policy.allowed_agents = ids;
  }
  const bad = policyFieldError(f);
  if (bad) { $('#policy-error').textContent = bad.msg; bad.el.focus(); return; }
  const max = f.elements.max_reads_per_day.value;
  if (max) policy.max_reads_per_day = Number(max);
  if (f.elements.not_after.value) policy.not_after = new Date(f.elements.not_after.value).toISOString();
  try {
    await api('PATCH', `/api/secrets/${encodeURIComponent(name)}`, { policy });
    policyDialog.close();
    await reload('secrets', 'audit');
  } catch (err) { $('#policy-error').textContent = friendly(err, { name }); }
});

// --- rotate dialog (KC-11) ---
const rotateDialog = $('#rotate-dialog');
$('#rotate-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = e.target;
  const name = f.dataset.name;
  const value = f.elements.value.value;
  if (!value) { $('#rotate-error').textContent = 'Enter the new value.'; return; }
  const submit = f.querySelector('button[type=submit]');
  submit.disabled = true; $('#rotate-progress').textContent = 'Sealing in TEE (a few seconds)…';
  try {
    await api('PUT', `/api/secrets/${encodeURIComponent(name)}`, { value });
    f.reset(); rotateDialog.close();
    await reload('secrets', 'audit');
  } catch (err) { $('#rotate-error').textContent = friendly(err, { name }); }
  finally { submit.disabled = false; $('#rotate-progress').textContent = ''; }
});
document.querySelectorAll('dialog [data-close]').forEach((b) => b.addEventListener('click', () => {
  const dlg = b.closest('dialog');
  dlg.querySelector('form').reset();
  dlg.querySelectorAll('.error').forEach((x) => { x.textContent = ''; });
  dlg.close();
}));
// Closing with Esc must also discard a typed secret value.
[policyDialog, rotateDialog].forEach((d) => d.addEventListener('close', () => d.querySelector('form').reset()));

$('#secrets').addEventListener('click', async (e) => {
  const btn = e.target.closest('button[data-act]');
  if (!btn) return;
  const card = btn.closest('.trigger-card');
  const name = card.dataset.name;
  const act = btn.dataset.act;
  try {
    if (act === 'rotate') {
      $('#rotate-secret-name').textContent = name;
      $('#rotate-form').dataset.name = name;
      $('#rotate-error').textContent = '';
      rotateDialog.showModal();
      return;
    } else if (act === 'policy') {
      await openPolicyEditor(name);
      return;
    } else if (act === 'release') {
      const toPlaintext = card.dataset.release === 'in_tee_only';
      if (toPlaintext) {
        // Downgrading protection is the one edit on this card that widens who
        // can see the value — make it deliberate (KC-10).
        const detail = await api('GET', `/api/secrets/${encodeURIComponent(name)}`);
        const scope = detail.policy.allowed_agents
          ? `${detail.policy.allowed_agents.length} permitted agent(s): ${detail.policy.allowed_agents.map(agentName).join(', ')}`
          : 'EVERY agent key in this vault (the policy currently allows all agents)';
        const ok = confirm(
          `Make ${name} plaintext-readable?\n\n` +
          `Right now only your attached Lit Actions can decrypt it, inside the TEE. After this change the decrypted value will be handed to ${scope}` +
          `${detail.disabled ? '\n\n(The secret is disabled, so no reads happen until you enable it.)' : ''}\n\n` +
          `You can switch back to in-TEE-only at any time.`,
        );
        if (!ok) return;
      }
      await api('PATCH', `/api/secrets/${encodeURIComponent(name)}`, { release: toPlaintext ? 'plaintext' : 'in_tee_only' });
    } else if (act === 'toggle') {
      await api('PATCH', `/api/secrets/${encodeURIComponent(name)}`, { disabled: card.dataset.disabled !== '1' });
    } else if (act === 'delete') {
      if (!confirm(`Delete ${name} and all its versions?\n\nAgents will get "not found" from now on. Past access-log entries keep the name, marked as deleted.`)) return;
      await api('DELETE', `/api/secrets/${encodeURIComponent(name)}`);
    }
    await reload('secrets', 'audit');
  } catch (err) { alert(friendly(err, { name })); }
});

// A number/datetime control with a half-typed value ("1e", a date without a
// time) serializes as "" while reporting validity.badInput. Treating that as
// "no limit" would silently create an unlimited secret, so check explicitly.
function policyFieldError(form) {
  const max = form.elements.max_reads_per_day;
  const until = form.elements.not_after;
  if (max && !max.checkValidity()) return { el: max, msg: 'Max reads must be a whole number of at least 1, or left empty for unlimited.' };
  if (max && max.value && !(Number.isInteger(Number(max.value)) && Number(max.value) >= 1)) return { el: max, msg: 'Max reads must be a whole number of at least 1, or left empty for unlimited.' };
  if (until && !until.checkValidity()) return { el: until, msg: 'Enter a complete date and time for "Not after", or leave it empty for no expiry.' };
  if (until && until.value && Number.isNaN(new Date(until.value).getTime())) return { el: until, msg: 'Enter a complete date and time for "Not after", or leave it empty for no expiry.' };
  return null;
}

// --- add-secret form (KC-12, KC-13) ---
const secretForm = $('#secret-form');
function resetSecretForm() {
  secretForm.reset();
  $('#secret-error').textContent = '';
  secretForm.querySelectorAll(':invalid').forEach((i) => i.setCustomValidity(''));
}
$('#new-secret-btn').addEventListener('click', () => {
  const hidden = secretForm.classList.toggle('hidden');
  if (!hidden) secretForm.elements.name.focus();
});
$('#secret-cancel').addEventListener('click', () => { resetSecretForm(); secretForm.classList.add('hidden'); });
secretForm.addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = new FormData(e.target);
  const errEl = $('#secret-error');
  errEl.textContent = '';
  // Field-level validation with actionable text instead of the browser's
  // generic pattern-mismatch bubble.
  const nameInput = e.target.elements.name;
  if (!nameInput.checkValidity()) { errEl.textContent = ERROR_TEXT.invalid_name(); nameInput.focus(); return; }
  if (!f.get('value')) { errEl.textContent = ERROR_TEXT.invalid_value(); e.target.elements.value.focus(); return; }
  const bad = policyFieldError(e.target);
  if (bad) { errEl.textContent = bad.msg; bad.el.focus(); return; }
  const policy = {};
  if (f.get('max_reads_per_day')) policy.max_reads_per_day = Number(f.get('max_reads_per_day'));
  if (f.get('not_after')) policy.not_after = new Date(f.get('not_after')).toISOString();
  const submit = e.target.querySelector('button[type=submit]');
  const progress = $('#secret-progress');
  const firstSave = !($('#vault-status').classList.contains('ok'));
  submit.disabled = true; submit.textContent = 'Sealing in TEE…';
  progress.textContent = firstSave
    ? 'First secret: provisioning your vault on Chipotle (several on-chain transactions). This usually takes 30–60 seconds — leave this page open.'
    : 'Encrypting inside the TEE — usually a few seconds.';
  try {
    await api('POST', '/api/secrets', {
      name: f.get('name'), value: f.get('value'), kind: f.get('kind') || undefined,
      environment: f.get('environment') || undefined, release: f.get('release'), policy,
    });
    resetSecretForm();
    secretForm.classList.add('hidden');
    await reload('secrets', 'vault');
  } catch (err) {
    // Keep what the user typed so they can correct it.
    errEl.textContent = friendly(err, { name: f.get('name') });
  } finally {
    submit.disabled = false; submit.textContent = 'Seal & save'; progress.textContent = '';
  }
});

// ---------- agents ----------
loaders.agents = async () => {
  const list = await api('GET', '/api/agents');
  agentCache = list;
  const el = $('#agents');
  if (!list.length) { el.innerHTML = '<p class="muted">No agents yet. Mint one above, then give its key to your runtime agent.</p>'; return; }
  el.innerHTML = list.map((a) => `
    <div class="trigger-card ${a.revoked_at ? 'disabled' : ''}" data-id="${esc(a.id)}">
      <div class="trigger-title">
        <strong>${esc(a.name)}</strong>
        <span class="pill ${a.revoked_at ? '' : 'ok'}">${a.revoked_at ? 'revoked ' + esc(fmt(a.revoked_at)) : 'active'}</span>
      </div>
      <div class="muted">id <code>${esc(a.id)}</code> · minted ${esc(fmt(a.created_at))} · last seen ${esc(fmt(a.last_seen_at))}</div>
      ${a.revoked_at ? '' : '<div class="row actions"><button data-act="revoke" class="compact danger">Revoke…</button></div>'}
    </div>`).join('');
};

$('#agents').addEventListener('click', async (e) => {
  const btn = e.target.closest('button[data-act=revoke]');
  if (!btn) return;
  const card = btn.closest('.trigger-card');
  const id = card.dataset.id;
  const name = card.querySelector('strong').textContent;
  // Match the documented guarantee (KC-09): Keychain denies immediately;
  // Chipotle's replicas may take a few minutes to catch up.
  if (!confirm(`Revoke the key for "${name}"?\n\nThe Keychain API rejects it immediately, so no new grants can be issued. The key is also removed from Chipotle; allow up to ~5 minutes for every Chipotle replica to stop accepting it. This cannot be undone — mint a new key if the agent needs access again.`)) return;
  try { await api('DELETE', `/api/agents/${id}`); await reload('agents', 'secrets'); } catch (err) { alert(friendly(err)); }
});

// One-time key handoff (KC-07): contained, copyable, with an explicit "done".
function renderNewAgent(a) {
  const box = $('#agent-new');
  const example = $('.example-secret')?.textContent || 'OPENAI_API_KEY';
  box.dataset.copied = '';
  box.classList.remove('hidden');
  box.innerHTML = `
    <div class="section-header"><b>Key for ${esc(a.name)}</b><span class="pill warn">shown once</span></div>
    <p class="muted">This is the only time the key is displayed. If you lose it, revoke this agent and mint a new one.</p>
    <div class="keybox">
      <input id="new-key" type="password" readonly value="${esc(a.usage_api_key)}" aria-label="agent usage key" />
      <button type="button" class="compact ghost" data-key="reveal">Show</button>
      <button type="button" class="compact" data-key="copy">Copy</button>
    </div>
    <div id="copy-status" class="muted" role="status" aria-live="polite"></div>
    <p class="muted">In the agent's environment:</p>
<pre><code>export LIT_AGENT_KEYCHAIN_KEY=&lt;paste the key&gt;
npm install @lit-protocol/keychain</code></pre>
<pre><code>import { LitAgentKeychain } from '@lit-protocol/keychain';
const keychain = new LitAgentKeychain({ usageApiKey: process.env.LIT_AGENT_KEYCHAIN_KEY, baseUrl: '${esc(window.location.origin)}' });
const value = await keychain.get('${esc(example)}');   // 403 with a reason if policy denies</code></pre>
    <p class="muted">Verify without printing the secret: <code>node -e "…keychain.get('${esc(example)}').then(v =&gt; console.log('ok, ' + v.length + ' chars'))"</code>. The read appears in the access log below.</p>
    <div class="row"><button type="button" class="compact ghost" data-key="done">Done — I've stored the key</button></div>`;
}
$('#agent-new').addEventListener('click', async (e) => {
  const btn = e.target.closest('button[data-key]');
  if (!btn) return;
  const box = $('#agent-new');
  const input = $('#new-key');
  const status = $('#copy-status');
  if (btn.dataset.key === 'reveal') {
    const show = input.type === 'password';
    input.type = show ? 'text' : 'password';
    btn.textContent = show ? 'Hide' : 'Show';
  } else if (btn.dataset.key === 'copy') {
    try {
      await navigator.clipboard.writeText(input.value);
      box.dataset.copied = '1';
      status.textContent = 'Copied to clipboard.';
    } catch {
      input.type = 'text'; input.focus(); input.select();
      status.textContent = 'Clipboard access was blocked — the key is selected, press ⌘C / Ctrl+C to copy it.';
    }
  } else if (btn.dataset.key === 'done') {
    if (!box.dataset.copied && !confirm('You have not used the Copy button. Dismiss anyway? The key cannot be shown again.')) return;
    box.classList.add('hidden'); box.innerHTML = '';
  }
});

$('#agent-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = new FormData(e.target);
  const btn = e.target.querySelector('button');
  btn.disabled = true; btn.textContent = 'Minting…';
  try {
    const a = await api('POST', '/api/agents', { name: f.get('name') });
    renderNewAgent(a);
    e.target.reset();
    await reload('agents', 'vault', 'secrets');
    $('#agent-new').scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  } catch (err) { alert(friendly(err, { name: f.get('name') })); } finally { btn.disabled = false; btn.textContent = 'Mint key'; }
});

// ---------- setup-agent tokens (KC-05) ----------
loaders['setup-tokens'] = async () => {
  const list = await api('GET', '/api/setup-tokens');
  const el = $('#setup-tokens');
  if (!list.length) { el.innerHTML = '<p class="muted">No setup agents authorized. An agent following <a href="/SKILL.md">/SKILL.md</a> will send you to the authorization page.</p>'; return; }
  el.innerHTML = list.map((t) => `
    <div class="trigger-card ${t.revoked_at ? 'disabled' : ''}" data-id="${esc(t.id)}">
      <div class="trigger-title">
        <strong>${esc(t.label)}</strong>
        <span class="pill ${t.revoked_at ? '' : 'warn'}">${t.revoked_at ? 'revoked ' + esc(fmt(t.revoked_at)) : 'full access'}</span>
      </div>
      <div class="muted">token id <code>…${esc(t.id.slice(-10))}</code> · authorized ${esc(fmt(t.created_at))} · last used ${esc(fmt(t.last_used_at))} · no expiry</div>
      ${t.revoked_at ? '' : '<div class="row actions"><button data-act="revoke" class="compact danger">Revoke…</button></div>'}
    </div>`).join('');
};
$('#setup-tokens').addEventListener('click', async (e) => {
  const btn = e.target.closest('button[data-act=revoke]');
  if (!btn) return;
  const card = btn.closest('.trigger-card');
  if (!confirm(`Revoke setup agent "${card.querySelector('strong').textContent}"?\n\nIts bearer token stops working immediately (HTTP 401). Runtime agent keys it minted are NOT affected — revoke those separately in the Agents section if needed.`)) return;
  try { await api('DELETE', `/api/setup-tokens/${encodeURIComponent(card.dataset.id)}`); await loadSection('setup-tokens'); } catch (err) { alert(friendly(err)); }
});
$('#setup-tokens-refresh').addEventListener('click', () => loadSection('setup-tokens'));

// ---------- tenant actions ----------
loaders.actions = async () => {
  const list = await api('GET', '/api/actions');
  const el = $('#actions');
  if (!list.length) { el.innerHTML = '<p class="muted">No customer actions attached.</p>'; return; }
  el.innerHTML = list.map((a) => `
    <div class="trigger-card" data-id="${esc(a.id)}">
      <div class="trigger-title"><strong>${esc(a.name)}</strong><button data-act="detach" class="compact danger">Detach</button></div>
      <div class="muted"><code>${esc(a.cid)}</code> · ${fmt(a.created_at)}</div>
    </div>`).join('');
};

$('#actions').addEventListener('click', async (e) => {
  const btn = e.target.closest('button[data-act=detach]');
  if (!btn) return;
  const id = btn.closest('.trigger-card').dataset.id;
  try { await api('DELETE', `/api/actions/${id}`); await loadSection('actions'); } catch (err) { alert(friendly(err)); }
});

$('#action-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = new FormData(e.target);
  try {
    await api('POST', '/api/actions', { cid: f.get('cid'), name: f.get('name') || undefined });
    e.target.reset();
    await reload('actions', 'vault');
  } catch (err) { alert(friendly(err)); }
});

// ---------- audit ----------
const REASON_TEXT = {
  secret_disabled: 'secret is disabled',
  release_not_plaintext: 'secret is in-TEE-only',
  agent_not_allowed: 'agent not in allowlist',
  policy_expired: 'policy expiry passed',
  rate_limited: 'read quota reached',
  secret_not_found: 'no such secret',
  version_not_found: 'no such version',
};
loaders.audit = async () => {
  const rows = await api('GET', '/api/audit?limit=100');
  $('#audit-state').innerHTML = '';
  $('#audit tbody').innerHTML = rows.length
    ? rows.map((r) => `<tr class="${r.decision}">
        <td>${esc(fmt(r.created_at))}</td><td>${esc(r.event)}</td>
        <td><code>${esc(r.secret_name || '—')}</code>${r.secret_deleted ? ' <span class="pill" title="This secret has since been deleted">deleted</span>' : ''}</td>
        <td>${esc(r.agent_name || '—')}${r.agent_id ? ` <span class="muted">…${esc(shortId(r.agent_id))}</span>` : ''}</td>
        <td>${esc(r.decision)}</td><td>${esc(REASON_TEXT[r.reason] || r.reason || '')}</td></tr>`).join('')
    : '<tr><td colspan="6" class="muted">No access yet.</td></tr>';
};
$('#audit-refresh').addEventListener('click', () => loadSection('audit'));

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
  } catch (err) {
    if (err.code !== 'unauthorized') {
      $('#me').innerHTML = `<span class="load-error">Couldn't load your account: ${esc(friendly(err))} <button class="ghost compact" onclick="location.reload()">Reload</button></span>`;
    }
    return;
  }
  // Sections load independently; one failing never blanks the others.
  await reload('vault', 'secrets', 'agents', 'setup-tokens', 'actions', 'audit');
})();
