/**
 * Shared UI utilities — leaf module with no app-level imports.
 * Toast notifications, clipboard, error formatting, HTML escaping, modals, confirm dialogs.
 */

// ----- Error classification -----

/**
 * Classify an error into a user-friendly category.
 * @param {Error|string|*} e
 * @returns {{ type: 'auth'|'network'|'server'|'unknown', message: string }}
 */
export function classifyError(e) {
  const raw = (e && e.message) ? e.message : String(e);
  const lower = raw.toLowerCase();

  // Auth: only specific phrases, not bare "session"/"expired" which collide with
  // contract revert reasons and ethers messages that mention those words.
  if (
    /\bunauthorized\b/.test(lower) ||
    /\b401\b/.test(lower) ||
    /\b403\b/.test(lower) ||
    lower.includes('session expired') ||
    lower.includes('token expired') ||
    lower.includes('api key')
  ) {
    return { type: 'auth', message: 'Session expired — please log in again.' };
  }
  if (lower.includes('failed to fetch') || lower.includes('networkerror') || lower.includes('err_connection')) {
    return { type: 'network', message: 'Connection lost — check your network and try again.' };
  }
  if (/\b500\b/.test(lower) || /\b502\b/.test(lower) || /\b503\b/.test(lower) || lower.includes('internal server')) {
    return { type: 'server', message: 'Something went wrong on the server. Please try again.' };
  }
  return { type: 'unknown', message: raw };
}

/**
 * Format an error for display. Uses classification for known patterns,
 * falls back to raw message for unrecognized errors.
 * @param {Error|string|*} e
 * @returns {string}
 */
export function formatError(e) {
  const classified = classifyError(e);
  if (classified.type !== 'unknown') return classified.message;
  return (e && e.message) ? e.message : String(e);
}

/**
 * Log a structured error to the console.
 * @param {string} context - What was being attempted
 * @param {Error|string|*} e - The error
 * @param {Object} [extra] - Additional context
 */
export function logError(context, e, extra) {
  const classified = classifyError(e);
  console.error(`[dashboard] ${context}`, {
    type: classified.type,
    message: classified.message,
    raw: (e && e.message) ? e.message : String(e),
    ...extra,
  });
}

// ----- HTML escaping -----

export function escapeHtml(s) {
  if (s == null) return '';
  const div = document.createElement('div');
  div.textContent = s;
  return div.innerHTML;
}

// ----- Clipboard -----

/**
 * Copy text to clipboard with visual feedback on a button element.
 * @param {string} text - Text to copy
 * @param {HTMLElement} [buttonEl] - Button to show feedback on
 * @param {string} [feedbackProp='textContent'] - Property to set feedback on ('textContent' or 'title')
 */
export async function copyToClipboard(text, buttonEl, feedbackProp = 'textContent') {
  try {
    await navigator.clipboard.writeText(text);
  } catch (_) {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.cssText = 'position:fixed;opacity:0';
    document.body.appendChild(ta);
    ta.select();
    document.execCommand('copy');
    document.body.removeChild(ta);
  }
  if (buttonEl) {
    const orig = buttonEl[feedbackProp];
    buttonEl[feedbackProp] = 'Copied!';
    setTimeout(() => { buttonEl[feedbackProp] = orig; }, 1500);
  }
}

// ----- Status messages (legacy inline — kept for compatibility) -----

export function showStatus(elId, message, type = 'info') {
  const el = document.getElementById(elId);
  if (!el) return;
  el.textContent = message;
  el.className = 'status ' + type;
  el.style.display = 'block';
}

export function hideStatus(elId) {
  const el = document.getElementById(elId);
  if (el) el.style.display = 'none';
}

// ----- Modal (Add / Edit) -----

export function openModal(title, bodyHTML, footerHTML) {
  const overlay = document.getElementById('modal-overlay');
  const titleEl = document.getElementById('modal-title');
  const body = document.getElementById('modal-body');
  const footer = document.getElementById('modal-footer');
  if (!overlay || !titleEl || !body || !footer) return;
  titleEl.textContent = title;
  body.innerHTML = bodyHTML;
  footer.innerHTML = footerHTML;
  overlay.classList.add('is-open');
  overlay.setAttribute('aria-hidden', 'false');
  const firstInput = body.querySelector('input, select, textarea');
  if (firstInput) firstInput.focus();
}

export function closeModal() {
  const overlay = document.getElementById('modal-overlay');
  if (overlay) {
    overlay.classList.remove('is-open');
    overlay.setAttribute('aria-hidden', 'true');
  }
}

export function initModalClose() {
  document.getElementById('modal-close-btn')?.addEventListener('click', closeModal);
  document.getElementById('modal-overlay')?.addEventListener('click', (e) => {
    if (e.target.id === 'modal-overlay') closeModal();
  });
}

// ----- Confirm delete -----

let _confirmResolve = null;

export function confirmDelete(message) {
  return new Promise((resolve) => {
    _confirmResolve = resolve;
    const overlay = document.getElementById('confirm-overlay');
    const msgEl = document.getElementById('confirm-message');
    if (msgEl) msgEl.textContent = message || 'Are you sure you want to delete this item?';
    if (overlay) {
      overlay.classList.add('is-open');
      overlay.setAttribute('aria-hidden', 'false');
    }
  });
}

function closeConfirm(confirmed) {
  const overlay = document.getElementById('confirm-overlay');
  if (overlay) {
    overlay.classList.remove('is-open');
    overlay.setAttribute('aria-hidden', 'true');
  }
  if (_confirmResolve) {
    _confirmResolve(!!confirmed);
    _confirmResolve = null;
  }
}

export function initConfirmClose() {
  document.getElementById('confirm-cancel-btn')?.addEventListener('click', () => closeConfirm(false));
  document.getElementById('confirm-close-btn')?.addEventListener('click', () => closeConfirm(false));
  document.getElementById('confirm-delete-btn')?.addEventListener('click', () => closeConfirm(true));
  document.getElementById('confirm-overlay')?.addEventListener('click', (e) => {
    if (e.target.id === 'confirm-overlay') closeConfirm(false);
  });
}

// ----- Action progress modal (non-dismissible) -----

let _actionProgressPreviousFocus = null;

export function showActionProgress(title, description) {
  const overlay = document.getElementById('action-overlay');
  const titleEl = document.getElementById('action-title');
  const descEl = document.getElementById('action-desc');
  if (!overlay || !titleEl || !descEl) return;
  titleEl.textContent = title || 'Working…';
  descEl.textContent = description || '';
  _actionProgressPreviousFocus = document.activeElement;
  overlay.classList.add('is-open');
  overlay.setAttribute('aria-hidden', 'false');
  const dialog = overlay.querySelector('[role="dialog"]');
  if (dialog) dialog.focus();
}

export function closeActionProgress() {
  const overlay = document.getElementById('action-overlay');
  if (!overlay) return;
  overlay.classList.remove('is-open');
  overlay.setAttribute('aria-hidden', 'true');
  if (_actionProgressPreviousFocus && typeof _actionProgressPreviousFocus.focus === 'function') {
    try { _actionProgressPreviousFocus.focus(); } catch (_) { /* element may have been removed */ }
  }
  _actionProgressPreviousFocus = null;
}

// ----- Shared list render -----

export function renderMetadataList(listElId, emptyElId, items) {
  const list = document.getElementById(listElId);
  const empty = emptyElId ? document.getElementById(emptyElId) : null;
  if (!list) return;
  list.innerHTML = '';
  if (!items || items.length === 0) {
    if (empty) empty.style.display = 'block';
    return;
  }
  if (empty) empty.style.display = 'none';
  items.forEach((item) => {
    const li = document.createElement('li');
    li.className = 'list-item';
    li.innerHTML = '<span><strong>' + escapeHtml(item.name || '') + '</strong> (ID: ' + escapeHtml(String(item.id)) + ')</span><span class="mono">' + escapeHtml(item.description || '') + '</span>';
    list.appendChild(li);
  });
}

// ----- Icons -----

export const ICON_PENCIL = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z"/></svg>';
export const ICON_TRASH = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z"/></svg>';
export const ICON_COPY = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z"/></svg>';
