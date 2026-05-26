(() => {
  'use strict';

  const state = {
    triggers: [],
    selectedTrigger: null,
  };

  const $ = (id) => document.getElementById(id);
  const statusEl = $('status');

  function setStatus(message, cls = '') {
    statusEl.textContent = message || '';
    statusEl.className = `status ${cls}`.trim();
  }

  function clearStatus() {
    setStatus('');
  }

  function el(tag, attrs = {}, children = []) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(attrs)) {
      if (value === undefined || value === null || value === false) continue;
      if (key === 'className') node.className = value;
      else if (key === 'text') node.textContent = String(value);
      else if (key === 'dataset') {
        for (const [dataKey, dataValue] of Object.entries(value)) {
          node.dataset[dataKey] = String(dataValue);
        }
      } else if (key === 'type') node.type = value;
      else node.setAttribute(key, String(value));
    }
    for (const child of children) {
      if (child === undefined || child === null) continue;
      node.append(child instanceof Node ? child : document.createTextNode(String(child)));
    }
    return node;
  }

  function prettyJson(value) {
    if (value === undefined || value === null) return '';
    try {
      return JSON.stringify(value, null, 2);
    } catch (_err) {
      return String(value);
    }
  }

  function parseJsonField(value, label, fallback = {}) {
    const trimmed = (value || '').trim();
    if (!trimmed) return fallback;
    try {
      return JSON.parse(trimmed);
    } catch (_err) {
      throw new Error(`${label} must be valid JSON`);
    }
  }

  function optionalPositiveInteger(value, label) {
    const trimmed = String(value || '').trim();
    if (!trimmed) return null;
    const parsed = Number(trimmed);
    if (!Number.isInteger(parsed) || parsed < 1) {
      throw new Error(`${label} must be a positive integer`);
    }
    return parsed;
  }

  function apiError(prefix, resp) {
    return new Error(`${prefix} failed (HTTP ${resp.status})`);
  }

  async function fetchJson(url, options = {}, prefix = 'Request') {
    const resp = await fetch(url, {
      credentials: 'same-origin',
      ...options,
      headers: {
        ...(options.headers || {}),
      },
    });
    if (resp.status === 401) {
      window.location.href = '/login';
      return null;
    }
    if (!resp.ok) throw apiError(prefix, resp);
    if (resp.status === 204) return null;
    return resp.json();
  }

  async function loadProfile() {
    const me = await fetchJson('/api/me', {}, 'Load profile');
    if (!me) return null;
    $('who').textContent = `Signed in as ${me.email || 'unknown user'}`;
    return me;
  }

  async function refreshTriggers({ clear = true } = {}) {
    if (clear) clearStatus();
    const triggers = await fetchJson('/api/triggers', {}, 'List triggers');
    state.triggers = Array.isArray(triggers) ? triggers : [];
    renderTriggers();
  }

  function kindLabel(kind) {
    if (kind === 'chain_event') return 'chain event';
    return kind || 'unknown';
  }

  function webhookUrl(trigger) {
    return `${window.location.origin}/webhook/${trigger.id}`;
  }

  function summaryFor(trigger) {
    const cfg = trigger.config || {};
    if (trigger.kind === 'webhook') return `Webhook URL: ${webhookUrl(trigger)}`;
    if (trigger.kind === 'schedule') return `Cron: ${cfg.cron || 'not configured'}`;
    if (trigger.kind === 'chain_event') {
      const chain = cfg.chain || cfg.chain_id || 'unknown chain';
      const address = cfg.contract_address || cfg.address || 'unknown contract';
      const event = cfg.event_signature || 'unknown event';
      return `${chain} · ${address} · ${event}`;
    }
    return 'Unknown trigger type';
  }

  function renderTriggers() {
    const list = $('triggers-list');
    list.replaceChildren();
    $('list-empty').classList.toggle('hidden', state.triggers.length !== 0);

    for (const trigger of state.triggers) {
      const card = el('article', { className: 'trigger-card' });
      const title = el('div', { className: 'trigger-title' }, [
        el('h3', { text: trigger.name || 'Untitled trigger' }),
        el('span', {
          className: trigger.enabled ? 'pill ok' : 'pill muted',
          text: trigger.enabled ? 'enabled' : 'disabled',
        }),
      ]);

      const meta = el('dl', { className: 'meta-list' }, [
        el('div', {}, [el('dt', { text: 'Kind' }), el('dd', { text: kindLabel(trigger.kind) })]),
        el('div', {}, [el('dt', { text: 'Action CID' }), el('dd', { text: trigger.action_cid || 'pending' })]),
        el('div', {}, [el('dt', { text: 'Config' }), el('dd', { text: summaryFor(trigger) })]),
      ]);

      if (trigger.kind === 'chain_event') {
        const cfg = trigger.config || {};
        const filters = cfg.topic_filters ? prettyJson(cfg.topic_filters) : 'none';
        meta.append(el('div', {}, [el('dt', { text: 'Topic filters' }), el('dd', { text: filters })]));
      }

      const limits = [];
      if (trigger.max_runs_per_minute) limits.push(`${trigger.max_runs_per_minute}/min`);
      if (trigger.max_queued_runs) limits.push(`${trigger.max_queued_runs} queued`);
      meta.append(el('div', {}, [el('dt', { text: 'Limits' }), el('dd', { text: limits.join(', ') || 'service defaults' })]));

      const actions = el('div', { className: 'actions' });
      actions.append(actionButton('Edit', () => openEdit(trigger)));
      actions.append(actionButton('Runs', () => loadRuns(trigger)));
      if (trigger.kind === 'webhook') actions.append(actionButton('Copy webhook URL', () => copyWebhook(trigger)));
      actions.append(actionButton('Delete', () => deleteTrigger(trigger), 'danger'));

      card.append(title, meta, actions);
      list.append(card);
    }
  }

  function actionButton(text, handler, variant = '') {
    const button = el('button', { type: 'button', className: `secondary compact ${variant}`.trim(), text });
    button.addEventListener('click', handler);
    return button;
  }

  async function copyWebhook(trigger) {
    try {
      await navigator.clipboard.writeText(webhookUrl(trigger));
      setStatus('Webhook URL copied.', 'ok');
    } catch (_err) {
      setStatus('Could not copy automatically. Select the webhook URL from the trigger card.', 'error');
    }
  }

  function currentKeyMode(form) {
    const selected = form.querySelector('input[name="keyMode"]:checked');
    return selected ? selected.value : 'mint';
  }

  async function mintUsageKey({ chipotleUrl, adminKey, groupId }) {
    const adminKeyInput = $('create-admin-key');
    try {
      const url = String(chipotleUrl || '').trim();
      const key = String(adminKey || '').trim();
      const group = String(groupId || '').trim();
      if (!url) throw new Error('Chipotle URL is required to mint a scoped usage key');
      if (!key) throw new Error('Admin API key is required to mint a scoped usage key');
      if (!group) throw new Error('Group ID is required to mint a scoped usage key');

      const resp = await fetch(url, {
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          authorization: `Bearer ${key}`,
        },
        body: JSON.stringify({
          scope: {
            execute_in_groups: [group],
            create_pkp: false,
            manage_groups: false,
            manage_ipfs_ids_in_groups: false,
          },
        }),
      });

      if (!resp.ok) {
        throw new Error(`Chipotle usage-key mint failed (HTTP ${resp.status}). Check the admin key, group ID, and CORS settings.`);
      }
      const json = await resp.json();
      const usageKey = json.usage_api_key || json.usageApiKey || json.key;
      if (!usageKey) throw new Error('Chipotle response did not include a scoped usage key');
      return usageKey;
    } finally {
      adminKeyInput.value = '';
    }
  }

  function buildConfig(form, prefix) {
    const kind = form.get('kind');
    if (kind === 'webhook') return {};
    if (kind === 'schedule') {
      const cron = String(form.get('cron') || '').trim();
      if (!cron) throw new Error('Cron expression is required for schedule triggers');
      return { cron };
    }

    const chain = String(form.get('chain') || '').trim();
    const contract = String(form.get('contractAddress') || '').trim();
    const signature = String(form.get('eventSignature') || '').trim();
    if (!chain || !contract || !signature) {
      throw new Error('Chain, contract address, and event signature are required for chain event triggers');
    }
    const config = {
      chain,
      contract_address: contract,
      event_signature: signature,
    };
    const filtersText = String(form.get('topicFilters') || '').trim();
    if (filtersText) config.topic_filters = parseJsonField(filtersText, 'Topic filters JSON', []);
    const startBlock = String(form.get('startBlock') || '').trim();
    if (startBlock) config.start_block = /^0x/i.test(startBlock) ? startBlock : Number(startBlock);
    if (typeof config.start_block === 'number' && !Number.isSafeInteger(config.start_block)) {
      throw new Error(`${prefix} start block must be an integer or hex string`);
    }
    return config;
  }

  async function createTrigger(event) {
    event.preventDefault();
    const formEl = event.currentTarget;
    const form = new FormData(formEl);
    const button = $('create-btn');
    button.disabled = true;
    clearStatus();

    try {
      let usageApiKey;
      if (currentKeyMode(formEl) === 'manual') {
        usageApiKey = String(form.get('usageKey') || '').trim();
        if (!usageApiKey) throw new Error('A scoped usage key is required');
      } else {
        setStatus('Minting scoped usage key in browser…');
        usageApiKey = await mintUsageKey({
          chipotleUrl: form.get('chipotleUrl'),
          adminKey: form.get('adminKey'),
          groupId: form.get('groupId'),
        });
      }

      const payload = {
        name: String(form.get('name') || '').trim(),
        kind: form.get('kind'),
        action_code: String(form.get('actionCode') || ''),
        default_params: parseJsonField(form.get('defaultParams'), 'Default params JSON'),
        usage_api_key: usageApiKey,
        config: buildConfig(form, 'Chain event'),
      };
      const maxRate = optionalPositiveInteger(form.get('maxRunsPerMinute'), 'Max runs per minute');
      const maxQueued = optionalPositiveInteger(form.get('maxQueuedRuns'), 'Max queued runs');
      if (maxRate !== null) payload.max_runs_per_minute = maxRate;
      if (maxQueued !== null) payload.max_queued_runs = maxQueued;

      setStatus('Creating trigger…');
      await fetchJson('/api/triggers', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(payload),
      }, 'Create trigger');

      formEl.reset();
      $('create-admin-key').value = '';
      $('create-usage-key').value = '';
      $('create-default-params').value = '{}';
      updateKindPanels();
      updateKeyPanels();
      setStatus('Trigger created. Only the scoped usage key was sent to lit-triggers.', 'ok');
      await refreshTriggers({ clear: false });
    } catch (err) {
      setStatus(err.message || 'Could not create trigger', 'error');
    } finally {
      button.disabled = false;
      $('create-admin-key').value = '';
      $('create-usage-key').value = '';
    }
  }

  function openEdit(trigger) {
    state.selectedTrigger = trigger;
    $('edit-panel').classList.remove('hidden');
    $('edit-subtitle').textContent = `${kindLabel(trigger.kind)} · CID ${trigger.action_cid || 'pending'}`;
    $('edit-id').value = trigger.id;
    $('edit-name').value = trigger.name || '';
    $('edit-enabled').checked = Boolean(trigger.enabled);
    $('edit-config').value = prettyJson(trigger.config || {});
    $('edit-default-params').value = prettyJson(trigger.default_params || {});
    $('edit-action-code').value = trigger.action_code || '';
    $('edit-usage-key').value = '';
    $('edit-max-rate').value = trigger.max_runs_per_minute || '';
    $('edit-max-queued').value = trigger.max_queued_runs || '';
    $('edit-panel').scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  async function saveEdit(event) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const id = String(form.get('id') || '').trim();
    if (!id) return;
    const button = $('edit-save-btn');
    button.disabled = true;
    clearStatus();
    try {
      const payload = {
        name: String(form.get('name') || '').trim(),
        enabled: $('edit-enabled').checked,
        action_code: String(form.get('actionCode') || ''),
        config: parseJsonField(form.get('config'), 'Config JSON'),
        default_params: parseJsonField(form.get('defaultParams'), 'Default params JSON'),
      };
      const maxRate = optionalPositiveInteger(form.get('maxRunsPerMinute'), 'Max runs per minute');
      const maxQueued = optionalPositiveInteger(form.get('maxQueuedRuns'), 'Max queued runs');
      if (maxRate !== null) payload.max_runs_per_minute = maxRate;
      if (maxQueued !== null) payload.max_queued_runs = maxQueued;
      const usageKey = String(form.get('usageKey') || '').trim();
      if (usageKey) payload.usage_api_key = usageKey;

      await fetchJson(`/api/triggers/${encodeURIComponent(id)}`, {
        method: 'PATCH',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(payload),
      }, 'Update trigger');
      $('edit-usage-key').value = '';
      setStatus('Trigger updated.', 'ok');
      await refreshTriggers({ clear: false });
    } catch (err) {
      setStatus(err.message || 'Could not update trigger', 'error');
    } finally {
      button.disabled = false;
      $('edit-usage-key').value = '';
    }
  }

  async function deleteTrigger(trigger) {
    const ok = window.confirm(`Delete trigger "${trigger.name || trigger.id}"? This also deletes its run history.`);
    if (!ok) return;
    try {
      await fetchJson(`/api/triggers/${encodeURIComponent(trigger.id)}`, { method: 'DELETE' }, 'Delete trigger');
      setStatus('Trigger deleted.', 'ok');
      if (state.selectedTrigger && state.selectedTrigger.id === trigger.id) closeEdit();
      await refreshTriggers({ clear: false });
    } catch (err) {
      setStatus(err.message || 'Could not delete trigger', 'error');
    }
  }

  async function loadRuns(trigger) {
    clearStatus();
    $('runs-panel').classList.remove('hidden');
    $('runs-subtitle').textContent = trigger.name || trigger.id;
    $('runs-list').replaceChildren(el('p', { className: 'note', text: 'Loading runs…' }));
    $('runs-panel').scrollIntoView({ behavior: 'smooth', block: 'start' });
    try {
      const result = await fetchJson(`/api/triggers/${encodeURIComponent(trigger.id)}/runs?limit=20&offset=0`, {}, 'Load runs');
      renderRuns(result && Array.isArray(result.runs) ? result.runs : []);
    } catch (err) {
      $('runs-list').replaceChildren(el('p', { className: 'status error', text: err.message || 'Could not load runs' }));
    }
  }

  function renderRuns(runs) {
    const list = $('runs-list');
    list.replaceChildren();
    if (runs.length === 0) {
      list.append(el('p', { className: 'note', text: 'No runs recorded yet.' }));
      return;
    }

    for (const run of runs) {
      const card = el('article', { className: 'run-card' });
      card.append(el('div', { className: 'trigger-title' }, [
        el('h3', { text: run.status || 'unknown status' }),
        el('span', { className: 'pill muted', text: `attempt ${run.attempt || 1}` }),
      ]));
      card.append(el('dl', { className: 'meta-list' }, [
        el('div', {}, [el('dt', { text: 'Started' }), el('dd', { text: run.started_at || 'unknown' })]),
        el('div', {}, [el('dt', { text: 'Finished' }), el('dd', { text: run.finished_at || 'not finished' })]),
      ]));
      card.append(detailsBlock('Input', run.input));
      card.append(detailsBlock('Response', run.response));
      if (run.error) card.append(detailsBlock('Error', run.error));
      list.append(card);
    }
  }

  function detailsBlock(label, value) {
    const details = el('details', { className: 'json-details' });
    details.append(el('summary', { text: label }));
    details.append(el('pre', { text: typeof value === 'string' ? value : prettyJson(value) }));
    return details;
  }

  function closeEdit() {
    state.selectedTrigger = null;
    $('edit-panel').classList.add('hidden');
    $('edit-form').reset();
  }

  function updateKindPanels() {
    const kind = $('create-kind').value;
    document.querySelectorAll('[data-kind-panel]').forEach((panel) => {
      panel.classList.toggle('hidden', panel.dataset.kindPanel !== kind);
    });
  }

  function updateKeyPanels() {
    const mode = currentKeyMode($('create-form'));
    document.querySelectorAll('[data-key-panel]').forEach((panel) => {
      panel.classList.toggle('hidden', panel.dataset.keyPanel !== mode);
    });
  }

  function attachEvents() {
    $('create-form').addEventListener('submit', createTrigger);
    $('edit-form').addEventListener('submit', saveEdit);
    $('edit-cancel-btn').addEventListener('click', closeEdit);
    $('runs-close-btn').addEventListener('click', () => $('runs-panel').classList.add('hidden'));
    $('refresh-btn').addEventListener('click', () => refreshTriggers().catch((err) => setStatus(err.message || 'Refresh failed', 'error')));
    $('logout-btn').addEventListener('click', async () => {
      await fetch('/auth/logout', { method: 'POST', credentials: 'same-origin' });
      window.location.href = '/login';
    });
    $('create-kind').addEventListener('change', updateKindPanels);
    document.querySelectorAll('input[name="keyMode"]').forEach((input) => input.addEventListener('change', updateKeyPanels));
  }

  attachEvents();
  updateKindPanels();
  updateKeyPanels();
  loadProfile()
    .then((me) => {
      if (me) return refreshTriggers();
      return null;
    })
    .catch((err) => {
      $('who').textContent = 'Could not load profile';
      setStatus(err.message || 'Could not load profile', 'error');
    });
})();
