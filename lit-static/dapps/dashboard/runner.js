/**
 * Action Runner — CodeJar editor, execute Lit Actions, get IPFS CID.
 */

import { getApiKey, getEffectiveApiKey, getClient, getBaseUrl } from './auth.js';
import { hideStatus, formatError, logError } from './ui-utils.js';

let _codeJarEditor = null;
let _paramsJarEditor = null;

export async function initActionRunner() {
  const codeEl = document.getElementById('action-runner-code');
  const paramsEl = document.getElementById('action-runner-params');
  const btn = document.getElementById('btn-execute-lit-action');
  const btnGetCid = document.getElementById('btn-get-lit-action-ipfs-cid');
  const outputEl = document.getElementById('action-runner-output');

  if (!btn || !outputEl) return;

  let getCode;
  let getParams;

  try {
    const { CodeJar } = await import('https://cdn.jsdelivr.net/npm/codejar@4.2.0/+esm');
    const highlight = (editor) => {
      if (!window.Prism) return;
      const lang = editor.classList.contains('language-json') ? 'json' : 'javascript';
      const grammar = Prism.languages[lang];
      if (grammar) editor.innerHTML = Prism.highlight(editor.textContent, grammar, lang);
    };
    _codeJarEditor = CodeJar(codeEl, highlight, { tab: '  ' });
    _paramsJarEditor = CodeJar(paramsEl, highlight, { tab: '  ' });

    getCode = () => _codeJarEditor ? _codeJarEditor.toString() : (codeEl?.textContent ?? '');
    getParams = () => _paramsJarEditor ? _paramsJarEditor.toString() : (paramsEl?.textContent ?? '');
  } catch (e) {
    logError('codejar-init', e);
    if (codeEl) codeEl.setAttribute('contenteditable', 'true');
    if (paramsEl) paramsEl.setAttribute('contenteditable', 'true');
    getCode = () => (codeEl?.textContent ?? '');
    getParams = () => (paramsEl?.textContent ?? '');
  }

  btn.addEventListener('click', async () => {
    const accountKey = getApiKey();
    const usageKeyEl = document.getElementById('action-runner-usage-key');
    const usageKey = usageKeyEl?.value?.trim() ?? '';
    const apiKey = usageKey || getEffectiveApiKey();
    const code = (getCode ? getCode() : (codeEl?.textContent ?? '')).trim();
    const paramsRaw = (getParams ? getParams() : (paramsEl?.textContent ?? '')).trim();

    if (!accountKey) {
      hideStatus('action-runner-status');
      outputEl.textContent = 'Log in first to execute Lit Actions.';
      outputEl.className = 'action-runner-output error';
      return;
    }
    if (!code) {
      hideStatus('action-runner-status');
      outputEl.textContent = 'Enter Lit Action code.';
      outputEl.className = 'action-runner-output error';
      return;
    }

    let jsParams = null;
    if (paramsRaw) {
      try {
        jsParams = JSON.parse(paramsRaw);
      } catch (e) {
        outputEl.textContent = 'Invalid JSON in parameters: ' + formatError(e);
        outputEl.className = 'action-runner-output error';
        return;
      }
    }

    hideStatus('action-runner-status');
    outputEl.textContent = 'Executing…';
    outputEl.className = 'action-runner-output';
    btn.disabled = true;

    try {
      const client = await getClient();
      const result = await client.litAction({ apiKey, code, jsParams });
      outputEl.textContent = JSON.stringify(result, null, 2);
      outputEl.className = 'action-runner-output success';
    } catch (e) {
      logError('executeAction', e);
      outputEl.textContent = 'Error: ' + formatError(e);
      outputEl.className = 'action-runner-output error';
    } finally {
      btn.disabled = false;
    }
  });

  btnGetCid?.addEventListener('click', async () => {
    const code = getCode().trim();
    if (!code) {
      outputEl.textContent = 'Enter Lit Action code to get its IPFS CID.';
      outputEl.className = 'action-runner-output error';
      return;
    }
    outputEl.textContent = 'Fetching…';
    outputEl.className = 'action-runner-output';
    btnGetCid.disabled = true;
    try {
      const baseUrl = getBaseUrl().replace(/\/$/, '');
      const url = baseUrl + '/core/v1/get_lit_action_ipfs_id';
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(code),
      });
      const text = await res.text();
      if (!res.ok) throw new Error(text || res.status + ' ' + res.statusText);
      let cid = text;
      try {
        const parsed = JSON.parse(text);
        if (typeof parsed === 'string') cid = parsed;
      } catch (_) {}
      outputEl.textContent = cid;
      outputEl.className = 'action-runner-output success';
    } catch (e) {
      logError('getCid', e);
      outputEl.textContent = 'Error: ' + formatError(e);
      outputEl.className = 'action-runner-output error';
    } finally {
      btnGetCid.disabled = false;
    }
  });
}
