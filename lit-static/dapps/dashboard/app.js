/**
 * Lit Express Node Dashboard — entry point.
 * Imports all feature modules and orchestrates initialization.
 */

import { getApiKey, setTheme, getTheme, setApiKey, setOnAuthReady, updateStatCards, initLogin, setUsageKeyOverride, toggleOverrideEnabled, updateUsageKeyOverrideUI, clearOverrideState } from './auth.js';
import { initModalClose, initConfirmClose, showStatus, hideStatus, logError } from './ui-utils.js';
import { initBilling, loadBillingBalance } from './billing.js';
import { initGroups, loadGroups } from './groups.js';
import { initKeys, loadUsageKeys } from './keys.js';
import { initActions, loadActions } from './actions.js';
import { initWallets, loadWallets } from './wallets.js';
import { initActionRunner } from './runner.js';

// ----- Preload all tables (with error visibility) -----

async function preloadAllTables() {
  const apiKey = getApiKey();
  if (!apiKey) return;
  const results = await Promise.allSettled([
    loadGroups(),
    loadWallets(),
    loadUsageKeys(),
    loadActions(),
  ]);
  const failures = results.filter((r) => r.status === 'rejected');
  if (failures.length > 0) {
    failures.forEach((f) => logError('preload', f.reason));
    showStatus('login-status', 'Some data failed to load. Check individual sections for details.', 'error');
  }
}

// ----- Usage key override UI -----

function initUsageKeyOverride() {
  const overrideInput = document.getElementById('usage-key-override-input');
  const applyBtn = document.getElementById('usage-key-override-apply');
  const clearBtn = document.getElementById('usage-key-override-clear');
  if (applyBtn) {
    applyBtn.addEventListener('click', () => {
      const val = (overrideInput?.value || '').trim();
      if (!val) {
        showStatus('overview-status', 'Enter a usage API key to apply.', 'error');
        return;
      }
      setUsageKeyOverride(val);
      hideStatus('overview-status');
      showStatus('overview-status', 'Usage API key override applied. All dashboard operations will now use this key.', 'success');
      preloadAllTables();
    });
  }
  if (clearBtn) {
    clearBtn.addEventListener('click', () => {
      setUsageKeyOverride('');
      hideStatus('overview-status');
      showStatus('overview-status', 'Usage API key override cleared. Using account API key.', 'success');
      preloadAllTables();
    });
  }
  updateUsageKeyOverrideUI();
}

// ----- Sidebar scroll -----

const ACTION_RUNNER_ID = 'action-runner';
const MAIN_SECTION_IDS = ['overview', 'usage-keys', 'groups', 'actions', 'wallets'];

function setActionRunnerVisible(visible) {
  const actionRunnerSection = document.getElementById('section-' + ACTION_RUNNER_ID);
  if (actionRunnerSection) actionRunnerSection.style.display = visible ? '' : 'none';
  MAIN_SECTION_IDS.forEach((id) => {
    const el = document.getElementById('section-' + id);
    if (el) el.style.display = visible ? 'none' : '';
  });
}

function initSidebar() {
  document.querySelectorAll('.sidebar-link[data-scroll]').forEach((a) => {
    a.addEventListener('click', (e) => {
      e.preventDefault();
      const id = a.getAttribute('data-scroll');
      if (id === ACTION_RUNNER_ID) {
        setActionRunnerVisible(true);
        const el = document.getElementById('section-' + id);
        if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
      } else {
        setActionRunnerVisible(false);
        const el = document.getElementById('section-' + id);
        if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
      }
    });
  });
}

// ----- Header (theme toggle, account dropdown, sign out) -----

function closeAccountDropdown() {
  const wrap = document.getElementById('account-dropdown');
  const trigger = document.getElementById('account-dropdown-trigger');
  const panel = document.getElementById('account-dropdown-panel');
  if (wrap) wrap.classList.remove('is-open');
  if (trigger) trigger.setAttribute('aria-expanded', 'false');
  if (panel) panel.setAttribute('aria-hidden', 'true');
}

function initHeader() {
  const themeToggle = document.getElementById('theme-toggle');
  if (themeToggle) {
    themeToggle.addEventListener('click', () => {
      const next = getTheme() === 'dark' ? 'light' : 'dark';
      setTheme(next);
    });
  }

  const dropdown = document.getElementById('account-dropdown');
  const trigger = document.getElementById('account-dropdown-trigger');
  const panel = document.getElementById('account-dropdown-panel');
  if (trigger && panel) {
    trigger.addEventListener('click', (e) => {
      e.stopPropagation();
      const isOpen = dropdown?.classList.toggle('is-open');
      trigger.setAttribute('aria-expanded', isOpen ? 'true' : 'false');
      panel.setAttribute('aria-hidden', isOpen ? 'false' : 'true');
    });
  }

  document.addEventListener('click', (e) => {
    if (dropdown && !dropdown.contains(e.target)) closeAccountDropdown();
  });

  const toggleOverrideBtn = document.getElementById('toggle-usage-override-btn');
  if (toggleOverrideBtn) {
    toggleOverrideBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      closeAccountDropdown();
      toggleOverrideEnabled();
    });
  }

  const signoutBtn = document.getElementById('account-signout-btn');
  if (signoutBtn) {
    signoutBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      closeAccountDropdown();
      clearOverrideState();
      setApiKey('');
    });
  }
}

// ----- Auth ready callback -----

setOnAuthReady(() => {
  updateStatCards();
  preloadAllTables();
  loadBillingBalance();
  updateUsageKeyOverrideUI();
});

// ----- Init -----

function init() {
  setTheme(getTheme());
  initModalClose();
  initConfirmClose();
  initLogin();
  initKeys();
  initWallets();
  initGroups();
  initActions();
  initActionRunner();
  initSidebar();
  initHeader();
  initBilling();
  initUsageKeyOverride();
}

init();
