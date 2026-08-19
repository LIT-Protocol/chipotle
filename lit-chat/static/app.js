// Lit Chat frontend. Vanilla ES modules, no build step, no CDN imports.
// Model output goes through renderMarkdown (DOM-building, no innerHTML).

import { api, setCsrf, streamChat } from '/api.js';
import { renderMarkdown } from '/markdown.js';

const $ = (id) => document.getElementById(id);

const state = {
  me: null,
  models: [],
  conversations: [],
  currentId: null,
  currentVersion: 1,
  streaming: false,
  abortStream: null,
};

// ---------------------------------------------------------------------------
// Bootstrap

async function ensureSession() {
  try {
    state.me = await api.me();
  } catch (e) {
    if (e.code === 401) {
      await api.anonSession();
      state.me = await api.me();
    } else {
      throw e;
    }
  }
  setCsrf(state.me.csrf);
  renderAccount();
}

async function boot() {
  await ensureSession();
  state.models = await api.models();
  renderModelPicker();
  await refreshConversations();
  wireEvents();
  updateBudgetNote();
}

boot().catch((e) => {
  console.error('boot failed', e);
  const es = $('empty-state');
  es.classList.remove('hidden');
});

// ---------------------------------------------------------------------------
// Rendering

function renderAccount() {
  const status = $('account-status');
  const isAccount = state.me.kind === 'account';
  status.textContent = isAccount
    ? 'Signed in (email-derived key)'
    : 'Anonymous session — history lives and dies with this cookie';
  $('btn-login').classList.toggle('hidden', isAccount);
  $('btn-logout').classList.toggle('hidden', !isAccount);
}

function renderModelPicker() {
  const picker = $('model-picker');
  picker.replaceChildren();
  for (const m of state.models) {
    const opt = document.createElement('option');
    opt.value = m.model_id;
    const price = m.completion_usd_per_mtok != null ? ` · $${m.completion_usd_per_mtok}/Mtok out` : '';
    opt.textContent = `${m.display_name}${price}`;
    picker.appendChild(opt);
  }
  updateModelBadge();
}

function currentModel() {
  return state.models.find((m) => m.model_id === $('model-picker').value) || state.models[0];
}

function updateModelBadge() {
  const m = currentModel();
  const badge = $('model-badge');
  if (!m) { badge.textContent = ''; return; }
  badge.textContent = m.zdr ? 'External · ZDR' : 'External · opt-in';
  badge.title = 'External model — prompts leave the enclave; routed only to zero-data-retention providers';
  $('privacy-hint').textContent =
    'External model: prompts leave the enclave (zero-data-retention providers only)';
}

async function refreshConversations() {
  state.conversations = await api.listConversations();
  const nav = $('conversation-list');
  nav.replaceChildren();
  for (const c of state.conversations) {
    const item = document.createElement('div');
    item.className = 'conv-item' + (c.id === state.currentId ? ' active' : '');
    const title = document.createElement('button');
    title.className = 'conv-item-title';
    title.textContent = c.title || 'Untitled';
    title.addEventListener('click', () => openConversation(c.id));
    const actions = document.createElement('span');
    actions.className = 'conv-item-actions';
    const rename = document.createElement('button');
    rename.className = 'icon-btn';
    rename.title = 'Rename';
    rename.textContent = '✎';
    rename.addEventListener('click', () => renameConversation(c));
    const del = document.createElement('button');
    del.className = 'icon-btn danger';
    del.title = 'Delete';
    del.textContent = '×';
    del.addEventListener('click', () => deleteConversation(c));
    actions.append(rename, del);
    item.append(title, actions);
    nav.appendChild(item);
  }
}

function messageEl(role, content, ids = {}) {
  const wrap = document.createElement('div');
  wrap.className = `msg msg-${role}`;
  if (ids.id) wrap.dataset.id = ids.id;
  const bubble = document.createElement('div');
  bubble.className = 'msg-body';
  if (role === 'assistant') {
    bubble.appendChild(renderMarkdown(content));
  } else {
    // User text: plain text, preserve line breaks.
    for (const [idx, line] of String(content).split('\n').entries()) {
      if (idx > 0) bubble.appendChild(document.createElement('br'));
      bubble.appendChild(document.createTextNode(line));
    }
  }
  const tools = document.createElement('div');
  tools.className = 'msg-tools';
  const copy = document.createElement('button');
  copy.className = 'icon-btn';
  copy.title = 'Copy';
  copy.textContent = 'Copy';
  copy.addEventListener('click', () => navigator.clipboard.writeText(content));
  tools.appendChild(copy);
  if (role === 'assistant') {
    const regen = document.createElement('button');
    regen.className = 'icon-btn';
    regen.title = 'Regenerate';
    regen.textContent = 'Regenerate';
    regen.addEventListener('click', () => regenerate());
    tools.appendChild(regen);
  }
  wrap.append(bubble, tools);
  return wrap;
}

function setStreamingUi(on) {
  state.streaming = on;
  $('btn-send').classList.toggle('hidden', on);
  $('btn-stop').classList.toggle('hidden', !on);
  $('input').disabled = on;
}

function updateBudgetNote() {
  const note = $('budget-note');
  if (state.me.kind === 'anon' && state.me.anon_daily_token_budget > 0) {
    const used = state.me.anon_tokens_used_today || 0;
    const pct = Math.min(100, Math.round((used / state.me.anon_daily_token_budget) * 100));
    if (pct >= 60) {
      note.textContent =
        `Free anonymous budget: ${pct}% used today. "Save my history" lifts this later.`;
      note.classList.remove('hidden');
      return;
    }
  }
  note.classList.add('hidden');
}

// ---------------------------------------------------------------------------
// Conversations

async function openConversation(id) {
  state.currentId = id;
  const conv = state.conversations.find((c) => c.id === id);
  state.currentVersion = conv ? conv.version : 1;
  $('conv-title').textContent = conv?.title || 'Untitled';
  if (conv) $('model-picker').value = conv.model_id;
  updateModelBadge();
  $('empty-state').classList.add('hidden');
  const messages = await api.listMessages(id);
  const pane = $('messages');
  pane.replaceChildren();
  for (const m of messages) {
    if (m.role === 'system') continue;
    pane.appendChild(messageEl(m.role, m.content, { id: m.id }));
  }
  pane.scrollTop = pane.scrollHeight;
  await refreshConversations();
}

async function newConversation() {
  const model = currentModel();
  if (!model) return;
  const conv = await api.createConversation(model.model_id);
  state.currentId = conv.id;
  state.currentVersion = conv.version;
  $('conv-title').textContent = 'New conversation';
  $('messages').replaceChildren();
  $('empty-state').classList.add('hidden');
  await refreshConversations();
}

async function renameConversation(conv) {
  const title = prompt('Conversation title', conv.title || '');
  if (!title) return;
  try {
    await api.renameConversation(conv.id, title, conv.version);
  } catch (e) {
    if (e.slug === 'version_conflict') alert('Conversation changed elsewhere — reload and retry.');
    else alert('Rename failed.');
  }
  await refreshConversations();
  if (conv.id === state.currentId) $('conv-title').textContent = title;
}

async function deleteConversation(conv) {
  if (!confirm('Delete this conversation now? Encrypted copies persist in backups until backup expiry.')) return;
  await api.deleteConversation(conv.id);
  if (state.currentId === conv.id) {
    state.currentId = null;
    $('messages').replaceChildren();
    $('conv-title').textContent = 'New conversation';
    $('empty-state').classList.remove('hidden');
  }
  await refreshConversations();
}

// ---------------------------------------------------------------------------
// Streaming

async function send() {
  if (state.streaming) return;
  const input = $('input');
  const content = input.value.trim();
  if (!content) return;
  if (!state.currentId) await newConversation();

  input.value = '';
  const pane = $('messages');
  pane.appendChild(messageEl('user', content));
  const pending = document.createElement('div');
  pending.className = 'msg msg-assistant';
  const body = document.createElement('div');
  body.className = 'msg-body streaming';
  pending.appendChild(body);
  pane.appendChild(pending);
  pane.scrollTop = pane.scrollHeight;

  runStream({ conversation_id: state.currentId, content, model_id: currentModel()?.model_id }, pending, body);
}

function regenerate() {
  if (state.streaming || !state.currentId) return;
  const pane = $('messages');
  const last = pane.lastElementChild;
  if (last && last.classList.contains('msg-assistant')) last.remove();
  const pending = document.createElement('div');
  pending.className = 'msg msg-assistant';
  const body = document.createElement('div');
  body.className = 'msg-body streaming';
  pending.appendChild(body);
  pane.appendChild(body.parentElement);
  runStream({ conversation_id: state.currentId, regenerate: true }, pending, body);
}

function runStream(payload, pendingEl, bodyEl) {
  setStreamingUi(true);
  let acc = '';
  let rerenderQueued = false;
  const rerender = () => {
    if (rerenderQueued) return;
    rerenderQueued = true;
    requestAnimationFrame(() => {
      rerenderQueued = false;
      bodyEl.replaceChildren(renderMarkdown(acc));
      const pane = $('messages');
      pane.scrollTop = pane.scrollHeight;
    });
  };
  state.abortStream = streamChat(payload, {
    meta: () => {},
    delta: (d) => { acc += d.d || ''; rerender(); },
    done: async () => {
      bodyEl.classList.remove('streaming');
      // Replace the transient bubble with a full message element (adds tools).
      pendingEl.replaceWith(messageEl('assistant', acc));
      setStreamingUi(false);
      state.abortStream = null;
      state.me = await api.me().catch(() => state.me);
      updateBudgetNote();
      await refreshConversations();
      const conv = state.conversations.find((c) => c.id === state.currentId);
      if (conv?.title) $('conv-title').textContent = conv.title;
    },
    error: (e) => {
      bodyEl.classList.remove('streaming');
      const p = document.createElement('p');
      p.className = 'error';
      p.textContent = errorText(e);
      bodyEl.appendChild(p);
      setStreamingUi(false);
      state.abortStream = null;
    },
    finished: () => {
      if (state.streaming) {
        // Stream ended without done/error (e.g. stop): keep partial text.
        bodyEl.classList.remove('streaming');
        if (acc) pendingEl.replaceWith(messageEl('assistant', acc));
        else pendingEl.remove();
        setStreamingUi(false);
        state.abortStream = null;
      }
    },
  });
}

function errorText(e) {
  switch (e.error) {
    case 'anon_daily_budget_exhausted':
      return 'Free anonymous budget exhausted for today. "Save my history" to continue later.';
    case 'spend_breaker_accounts_only':
      return 'High demand: anonymous chat is paused; account holders only right now.';
    case 'spend_breaker_open':
      return 'Chat is temporarily paused (daily spend cap reached).';
    case 'no_inference_key':
      return 'No inference key is configured. An operator needs to visit the admin console.';
    default:
      return 'Something went wrong generating a response.';
  }
}

// ---------------------------------------------------------------------------
// Auth / account

function wireEvents() {
  $('new-chat').addEventListener('click', () => newConversation());
  $('btn-send').addEventListener('click', () => send());
  $('btn-stop').addEventListener('click', () => { state.abortStream?.(); });
  $('input').addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter' && !ev.shiftKey) {
      ev.preventDefault();
      send();
    }
  });
  $('model-picker').addEventListener('change', updateModelBadge);
  $('btn-privacy').addEventListener('click', () => $('privacy-modal').showModal());
  $('btn-login').addEventListener('click', () => {
    $('login-step-email').classList.remove('hidden');
    $('login-step-code').classList.add('hidden');
    $('login-error').classList.add('hidden');
    $('login-modal').showModal();
  });
  $('btn-send-code').addEventListener('click', async () => {
    const email = $('login-email').value.trim();
    if (!email) return;
    await api.requestCode(email);
    $('login-step-email').classList.add('hidden');
    $('login-step-code').classList.remove('hidden');
  });
  $('btn-verify-code').addEventListener('click', async () => {
    const code = $('login-code').value.trim();
    if (!code) return;
    try {
      const res = await api.verifyCode(code);
      $('login-modal').close();
      state.me = await api.me();
      setCsrf(state.me.csrf);
      renderAccount();
      updateBudgetNote();
      await refreshConversations();
      if (res.migrated_conversations > 0) {
        alert(`Signed in. ${res.migrated_conversations} conversation(s) migrated to your account.`);
      }
    } catch (e) {
      const el = $('login-error');
      el.textContent = e.slug === 'invalid_code' ? 'Wrong code.' : 'Code invalid or expired — request a new one.';
      el.classList.remove('hidden');
    }
  });
  $('btn-logout').addEventListener('click', async () => {
    await api.logout();
    location.reload();
  });
  $('btn-export').addEventListener('click', async () => {
    const data = await api.exportData();
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'lit-chat-export.json';
    a.click();
    URL.revokeObjectURL(a.href);
  });
  $('btn-delete-account').addEventListener('click', async () => {
    if (!confirm('Delete ALL conversations and this identity now? This cannot be undone. Encrypted copies persist in backups until backup expiry.')) return;
    await api.deleteAccount();
    location.reload();
  });
}
