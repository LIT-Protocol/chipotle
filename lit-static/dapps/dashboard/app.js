/**
 * Lit Express Node Dashboard — entry point.
 * Imports all feature modules and orchestrates initialization.
 */

import { isAuthenticated, setTheme, getTheme, logOut, setOnAuthReady, updateStatCards, initLogin, setUsageKeyOverride, toggleOverrideEnabled, updateUsageKeyOverrideUI, setChainSecuredRpcUrl, toggleChainSecuredRpcPanel, updateChainSecuredRpcUrlUI, getMode, getApiKey, convertToChainSecured, changeChainSecuredOwnership } from './auth.js';
import { initModalClose, initConfirmClose, showStatus, hideStatus, logError } from './ui-utils.js';
import { initBilling, handleBillingReturn } from './billing.js';
import { initAutoRecharge, openAutoRechargeModal } from './auto_recharge.js';
import { initGroups, loadGroups } from './groups.js';
import { initKeys, loadUsageKeys } from './keys.js';
import { initActions, loadActions } from './actions.js';
import { initWallets, loadWallets } from './wallets.js';
import { initActionRunner } from './runner.js';
import { initGvisorRunner } from './gvisor_runner.js';
import { initAbout } from './about.js';

// ----- Preload all tables (with error visibility) -----

async function preloadAllTables() {
  if (!isAuthenticated()) return;
  hideStatus('dashboard-status');
  const results = await Promise.allSettled([
    loadGroups(),
    loadWallets(),
    loadUsageKeys(),
    loadActions(),
  ]);
  const failures = results.filter((r) => r.status === 'rejected');
  if (failures.length > 0) {
    failures.forEach((f) => logError('preload', f.reason));
    showStatus('dashboard-status', 'Some data failed to load. Check individual sections for details.', 'error');
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

// Standalone sections get their own full-page view (like tabs); selecting one
// hides the scrollable main sections and every other standalone section.
const STANDALONE_SECTION_IDS = ['action-runner', 'gvisor-runner'];
const MAIN_SECTION_IDS = ['overview', 'usage-keys', 'groups', 'actions', 'wallets'];

/**
 * Show a single standalone section (by id) and hide the main sections, or —
 * when `standaloneId` is null — show the main sections and hide all
 * standalone ones.
 */
function showStandaloneSection(standaloneId) {
  STANDALONE_SECTION_IDS.forEach((id) => {
    const el = document.getElementById('section-' + id);
    if (el) el.style.display = id === standaloneId ? '' : 'none';
  });
  const showMain = standaloneId === null;
  MAIN_SECTION_IDS.forEach((id) => {
    const el = document.getElementById('section-' + id);
    if (el) el.style.display = showMain ? '' : 'none';
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
      showStandaloneSection(STANDALONE_SECTION_IDS.includes(id) ? id : null);
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

  const convertBtn = document.getElementById('convert-to-chainsecured-btn');
  if (convertBtn) {
    convertBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      closeAccountDropdown();
      convertToChainSecured();
    });
  }

  const changeOwnershipBtn = document.getElementById('change-ownership-btn');
  if (changeOwnershipBtn) {
    changeOwnershipBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      closeAccountDropdown();
      changeChainSecuredOwnership();
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

/**
 * Toggle the Convert-to-ChainSecured dropdown item. Only visible while signed
 * in with an API key (i.e. mode === 'api' AND a key is present).
 */
function refreshConvertVisibility() {
  const btn = document.getElementById('convert-to-chainsecured-btn');
  if (!btn) return;
  const showConvert = isAuthenticated() && getMode() === 'api' && !!getApiKey();
  btn.hidden = !showConvert;
}

/**
 * Toggle the Change-Ownership dropdown item. Only visible while signed in as a
 * ChainSecured account (sovereign mode) — the function reassigns the on-chain
 * admin wallet and is called directly by the current admin's signer.
 */
function refreshChangeOwnershipVisibility() {
  const btn = document.getElementById('change-ownership-btn');
  if (!btn) return;
  const show = isAuthenticated() && getMode() === 'sovereign';
  btn.hidden = !show;
}

// ----- Auth ready callback -----

function onAuthReady() {
  updateStatCards();
  preloadAllTables();
  updateUsageKeyOverrideUI();
  refreshConvertVisibility();
  refreshChangeOwnershipVisibility();
  updateChainSecuredRpcUrlUI();
  handleBillingReturn();
}

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
  initGvisorRunner();
  initSidebar();
  initHeader();
  initBilling();
  initAutoRecharge();
  document.getElementById('btn-auto-recharge')?.addEventListener(
    'click',
    () => openAutoRechargeModal(),
  );
  initUsageKeyOverride();
  initChainSecuredRpc();
  initAbout();

  // Register the auth-ready callback only after every init* call above has
  // attached its button listeners. Registering at module-eval time let
  // initLogin()'s synchronous updateAuthUI() (auth.js) fire _onAuthReady
  // mid-init for already-logged-in sessions — before button-disable wiring
  // ran, leaving a brief duplicate-click window. Because initLogin() already
  // ran (and skipped the then-null callback), trigger the flow once here for
  // sessions restored from a previous visit.
  setOnAuthReady(onAuthReady);
  if (isAuthenticated()) onAuthReady();
}

init();
