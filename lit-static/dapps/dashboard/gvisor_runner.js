/**
 * gVisor Runner (CPL-360) — edit a startup script, upload/paste an action
 * bundle (or reference one by CID), and execute it in the gVisor sandbox via
 * POST /lit_binary_action. Mirrors the Action Runner tab but targets the
 * any-language binary path instead of inline JS.
 */

import { getEffectiveApiKey, getClient, isAuthenticated } from './auth.js';
import { hideStatus, formatError, logError } from './ui-utils.js';

let _startupJarEditor = null;
let _paramsJarEditor = null;

/** Read a File as base64 (no data: prefix), matching the tar/tar.gz bytes the runner unpacks. */
function fileToBase64(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result || '';
      const comma = result.indexOf(',');
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.onerror = () => reject(reader.error || new Error('Failed to read file'));
    reader.readAsDataURL(file);
  });
}

export async function initGvisorRunner() {
  const startupEl = document.getElementById('gvisor-runner-startup');
  const paramsEl = document.getElementById('gvisor-runner-params');
  const bundleEl = document.getElementById('gvisor-runner-bundle');
  const fileEl = document.getElementById('gvisor-runner-bundle-file');
  const clearBtn = document.getElementById('gvisor-runner-bundle-clear');
  const cidEl = document.getElementById('gvisor-runner-cid');
  const bundlePanel = document.getElementById('gvisor-bundle-panel');
  const cidPanel = document.getElementById('gvisor-cid-panel');
  const btn = document.getElementById('btn-execute-gvisor-action');
  const outputEl = document.getElementById('gvisor-runner-output');

  if (!btn || !outputEl) return;

  let getStartup;
  let getParams;

  try {
    const { CodeJar } = await import('https://cdn.jsdelivr.net/npm/codejar@4.2.0/+esm');
    const highlight = (editor) => {
      if (!window.Prism) return;
      const lang = editor.classList.contains('language-json')
        ? 'json'
        : editor.classList.contains('language-bash')
          ? 'bash'
          : 'javascript';
      const grammar = Prism.languages[lang];
      if (grammar) editor.innerHTML = Prism.highlight(editor.textContent, grammar, lang);
    };
    _startupJarEditor = CodeJar(startupEl, highlight, { tab: '  ' });
    _paramsJarEditor = CodeJar(paramsEl, highlight, { tab: '  ' });

    getStartup = () => _startupJarEditor ? _startupJarEditor.toString() : (startupEl?.textContent ?? '');
    getParams = () => _paramsJarEditor ? _paramsJarEditor.toString() : (paramsEl?.textContent ?? '');
  } catch (e) {
    logError('gvisor-codejar-init', e);
    if (startupEl) startupEl.setAttribute('contenteditable', 'true');
    if (paramsEl) paramsEl.setAttribute('contenteditable', 'true');
    getStartup = () => (startupEl?.textContent ?? '');
    getParams = () => (paramsEl?.textContent ?? '');
  }

  // ----- Bundle source toggle (upload/paste vs cached CID) -----
  const syncBundleMode = () => {
    const mode = document.querySelector('input[name="gvisor-bundle-mode"]:checked')?.value ?? 'bundle';
    if (bundlePanel) bundlePanel.style.display = mode === 'bundle' ? '' : 'none';
    if (cidPanel) cidPanel.style.display = mode === 'cid' ? '' : 'none';
  };
  document.querySelectorAll('input[name="gvisor-bundle-mode"]').forEach((r) => {
    r.addEventListener('change', syncBundleMode);
  });
  syncBundleMode();

  // ----- File → base64 into the paste box -----
  fileEl?.addEventListener('change', async () => {
    const file = fileEl.files?.[0];
    if (!file) return;
    try {
      outputEl.textContent = 'Encoding bundle…';
      outputEl.className = 'action-runner-output';
      const b64 = await fileToBase64(file);
      if (bundleEl) bundleEl.value = b64;
      outputEl.textContent = '';
      outputEl.className = 'action-runner-output';
    } catch (e) {
      logError('gvisor-file-encode', e);
      outputEl.textContent = 'Error reading file: ' + formatError(e);
      outputEl.className = 'action-runner-output error';
    }
  });

  clearBtn?.addEventListener('click', () => {
    if (bundleEl) bundleEl.value = '';
    if (fileEl) fileEl.value = '';
  });

  btn.addEventListener('click', async () => {
    const usageKey = document.getElementById('gvisor-runner-usage-key')?.value?.trim() ?? '';
    const apiKey = usageKey || getEffectiveApiKey();
    const startupScript = (getStartup ? getStartup() : (startupEl?.textContent ?? '')).trim();
    const paramsRaw = (getParams ? getParams() : (paramsEl?.textContent ?? '')).trim();
    const mode = document.querySelector('input[name="gvisor-bundle-mode"]:checked')?.value ?? 'bundle';
    const bundle = (bundleEl?.value ?? '').trim();
    const checksum = (cidEl?.value ?? '').trim();

    if (!isAuthenticated()) {
      hideStatus('gvisor-runner-status');
      outputEl.textContent = 'Log in first to execute binary actions.';
      outputEl.className = 'action-runner-output error';
      return;
    }
    // ChainSecured has no account-level api key; users execute via a usage key
    // they minted from the contract. API mode falls back to the account key.
    if (!apiKey) {
      hideStatus('gvisor-runner-status');
      outputEl.textContent = 'Paste a Usage API Key above to execute binary actions.';
      outputEl.className = 'action-runner-output error';
      return;
    }
    if (mode === 'bundle' && !bundle) {
      hideStatus('gvisor-runner-status');
      outputEl.textContent = 'Upload or paste a bundle, or switch to "Use a cached bundle CID".';
      outputEl.className = 'action-runner-output error';
      return;
    }
    if (mode === 'cid' && !checksum) {
      hideStatus('gvisor-runner-status');
      outputEl.textContent = 'Enter a bundle CID, or switch to "Upload / paste a bundle".';
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

    hideStatus('gvisor-runner-status');
    outputEl.textContent = 'Executing…';
    outputEl.className = 'action-runner-output';
    btn.disabled = true;

    try {
      const client = await getClient();
      const result = await client.litBinaryAction({
        apiKey,
        bundle: mode === 'bundle' ? bundle : undefined,
        checksum: mode === 'cid' ? checksum : undefined,
        startupScript: startupScript || undefined,
        jsParams,
      });
      outputEl.textContent = JSON.stringify(result, null, 2);
      outputEl.className = 'action-runner-output success';
    } catch (e) {
      logError('executeBinaryAction', e);
      outputEl.textContent = 'Error: ' + formatError(e);
      outputEl.className = 'action-runner-output error';
    } finally {
      btn.disabled = false;
    }
  });
}
