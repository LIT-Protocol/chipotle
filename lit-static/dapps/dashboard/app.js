/**
 * Lit Express Node Dashboard — entry point.
 * Imports all feature modules and orchestrates initialization.
 */

import { isAuthenticated, setTheme, getTheme, logOut, setOnAuthReady, updateStatCards, initLogin, setUsageKeyOverride, toggleOverrideEnabled, updateUsageKeyOverrideUI, setChainSecuredRpcUrl, toggleChainSecuredRpcPanel, updateChainSecuredRpcUrlUI } from './auth.js';
import { initModalClose, initConfirmClose, showStatus, hideStatus, logError } from './ui-utils.js';
import { initBilling } from './billing.js';
import { initGroups, loadGroups } from './groups.js';
import { initKeys, loadUsageKeys } from './keys.js';
import { initActions, loadActions } from './actions.js';
import { initWallets, loadWallets } from './wallets.js';
import { initActionRunner } from './runner.js';

// ----- Preload all tables (with error visibility) -----

async function preloadAllTables() {
  if (!isAuthenticated()) return;
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

// ----- ChainSecured RPC URL UI (CPL-276) -----

function initChainSecuredRpc() {
  const input = document.getElementById('chainsecured-rpc-input');
  const applyBtn = document.getElementById('chainsecured-rpc-apply');
  const resetBtn = document.getElementById('chainsecured-rpc-reset');
  if (applyBtn) {
    applyBtn.addEventListener('click', () => {
      const val = (input?.value || '').trim();
      if (!val) {
        showStatus('overview-status', 'Enter an RPC URL.', 'error');
        return;
      }
      try {
        const u = new URL(val);
        if (u.protocol !== 'http:' && u.protocol !== 'https:') throw new Error('not http');
      } catch {
        showStatus('overview-status', 'Enter a valid http(s) RPC URL.', 'error');
        return;
      }
      setChainSecuredRpcUrl(val);
      hideStatus('overview-status');
      showStatus('overview-status', 'RPC URL updated. Dashboard will use this RPC for ChainSecured reads and writes.', 'success');
      preloadAllTables();
    });
  }
  if (resetBtn) {
    resetBtn.addEventListener('click', () => {
      setChainSecuredRpcUrl('');
      hideStatus('overview-status');
      showStatus('overview-status', 'RPC URL reset to default.', 'success');
      preloadAllTables();
    });
  }
  updateChainSecuredRpcUrlUI();
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

function setActiveSidebarLink(id) {
  document.querySelectorAll('.sidebar-link[data-scroll]').forEach((a) => {
    a.classList.toggle('is-active', a.getAttribute('data-scroll') === id);
  });
}

function initSidebar() {
  // Bind to any element with data-scroll (sidebar links, stat cards, empty-state CTAs).
  // Active-link styling stays sidebar-only via setActiveSidebarLink's selector.
  document.querySelectorAll('[data-scroll]').forEach((a) => {
    a.addEventListener('click', (e) => {
      e.preventDefault();
      const id = a.getAttribute('data-scroll');
      if (id === ACTION_RUNNER_ID) {
        setActionRunnerVisible(true);
      } else {
        setActionRunnerVisible(false);
      }
      const el = document.getElementById('section-' + id);
      if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
      setActiveSidebarLink(id);
    });
  });

  // Scroll-spy: highlight sidebar link for whichever section is in view.
  const sections = MAIN_SECTION_IDS
    .map((id) => document.getElementById('section-' + id))
    .filter(Boolean);
  if (sections.length === 0 || !('IntersectionObserver' in window)) return;

  const visible = new Map();
  const observer = new IntersectionObserver((entries) => {
    entries.forEach((entry) => {
      if (entry.isIntersecting) {
        visible.set(entry.target.id, entry.intersectionRatio);
      } else {
        visible.delete(entry.target.id);
      }
    });
    if (visible.size === 0) return;
    let bestId = null;
    let bestRatio = -1;
    visible.forEach((ratio, sectionId) => {
      if (ratio > bestRatio) {
        bestRatio = ratio;
        bestId = sectionId;
      }
    });
    if (bestId) setActiveSidebarLink(bestId.replace(/^section-/, ''));
  }, {
    rootMargin: '-80px 0px -55% 0px',
    threshold: [0, 0.1, 0.25, 0.5, 0.75, 1],
  });
  sections.forEach((el) => observer.observe(el));
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

  const toggleRpcBtn = document.getElementById('toggle-chainsecured-rpc-btn');
  if (toggleRpcBtn) {
    toggleRpcBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      closeAccountDropdown();
      toggleChainSecuredRpcPanel();
    });
  }

  const signoutBtn = document.getElementById('account-signout-btn');
  if (signoutBtn) {
    signoutBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      closeAccountDropdown();
      logOut();
    });
  }
}

// ----- Auth ready callback -----

setOnAuthReady(() => {
  updateStatCards();
  preloadAllTables();
  updateUsageKeyOverrideUI();
  updateChainSecuredRpcUrlUI();
});

// ----- Init -----

function showDevWarning() {
  if (location.hostname === 'dashboard.dev.litprotocol.com') {
    const overlay = document.getElementById('dev-warning-overlay');
    if (overlay) {
      overlay.classList.add('is-open');
      overlay.setAttribute('aria-hidden', 'false');
    }
    return true;
  }
  return false;
}

function init() {
  setTheme(getTheme());
  if (showDevWarning()) return;
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
  initChainSecuredRpc();
}

init();
