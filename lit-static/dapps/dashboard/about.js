/**
 * About Dashboard modal — shows dashboard + API versions and a GitHub link.
 *
 * The dashboard commit is baked in at deploy time by replacing the
 * `__LIT_DASHBOARD_GIT_COMMIT__` placeholder below (see
 * .github/workflows/deploy-static.yml and deploy-prod-3-static.yml). When
 * the placeholder is left unreplaced (local dev), we display "local".
 *
 * The API version is fetched from `GET /core/v1/version`.
 */

import { getBaseUrl } from './auth.js';
import { openModal, closeModal, escapeHtml, logError } from './ui-utils.js';

const GITHUB_REPO_URL = 'https://github.com/LIT-Protocol/chipotle';
const DASHBOARD_COMMIT_RAW = '__LIT_DASHBOARD_GIT_COMMIT__';
// Reassembled at runtime so the deploy-time `sed` replacement does not also
// rewrite this sentinel (it only matches the exact contiguous placeholder).
const PLACEHOLDER = '__LIT_' + 'DASHBOARD_GIT_COMMIT__';

function dashboardCommit() {
  return DASHBOARD_COMMIT_RAW === PLACEHOLDER ? '' : DASHBOARD_COMMIT_RAW;
}

// Extract a hex SHA from a `git describe` style string. Returns '' if none.
// Accepts a full 40-char hash, a 7-12 char short hash, or the `g<sha>` suffix
// produced by `git describe` after a tag (e.g. "v1.2.3-4-gabcdef0").
function extractSha(s) {
  if (!s) return '';
  const describeMatch = s.match(/-g([0-9a-f]{7,40})(?:-[^ ]*)?$/i);
  if (describeMatch) return describeMatch[1];
  const bareMatch = s.match(/^[0-9a-f]{7,40}$/i);
  if (bareMatch) return bareMatch[0];
  return '';
}

// Render a commit value as a (possibly-linked) short label.
// `emptyLabel` controls the fallback when `commit` is empty — "local" for the
// dashboard build placeholder, "unknown" for an API response missing the field.
function commitLinkHtml(commit, emptyLabel) {
  if (!commit) return `<span class="mono">${escapeHtml(emptyLabel)}</span>`;
  const sha = extractSha(commit);
  // When a SHA is present, the label is the short SHA so it matches the link
  // target. Otherwise fall back to truncating the raw string.
  const label = sha
    ? sha.slice(0, 12)
    : (commit.length > 16 ? commit.slice(0, 12) : commit);
  if (!sha) return `<span class="mono">${escapeHtml(label)}</span>`;
  const href = `${GITHUB_REPO_URL}/commit/${encodeURIComponent(sha)}`;
  return `<a class="mono" href="${escapeHtml(href)}" target="_blank" rel="noopener">${escapeHtml(label)}</a>`;
}

async function fetchApiVersion() {
  const baseUrl = getBaseUrl().replace(/\/$/, '');
  const res = await fetch(`${baseUrl}/core/v1/version`, { method: 'GET' });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return await res.json();
}

function bodyHtml(apiVersionHtml) {
  const dashHtml = commitLinkHtml(dashboardCommit(), 'local');
  return `
    <p style="margin:0 0 0.75rem;">
      The Chipotle Dashboard is a web interface for managing your Lit accounts, wallets, groups, IPFS actions, and usage API keys.
    </p>
    <p style="margin:0 0 1rem;">
      Built by <a href="https://litprotocol.com" target="_blank" rel="noopener">Lit Protocol</a>. Source available on
      <a href="${escapeHtml(GITHUB_REPO_URL)}" target="_blank" rel="noopener">GitHub</a>.
    </p>
    <dl class="about-versions">
      <dt>Dashboard version</dt>
      <dd>${dashHtml}</dd>
      <dt>API version</dt>
      <dd id="about-api-version">${apiVersionHtml}</dd>
    </dl>
  `;
}

const FOOTER_HTML = `<button type="button" class="btn btn-primary" id="about-close-btn">Close</button>`;

function openAboutModal() {
  openModal('About Dashboard', bodyHtml('<span class="mono">Loading…</span>'), FOOTER_HTML);
  // The shared modal X and overlay-click are wired by initModalClose();
  // we only need to wire our explicit footer Close button.
  document.getElementById('about-close-btn')?.addEventListener('click', closeModal);

  fetchApiVersion().then((v) => {
    const el = document.getElementById('about-api-version');
    if (!el) return;
    const commit = (v && v.commit_version) ? String(v.commit_version) : '';
    const pkgVersion = (v && v.version) ? String(v.version) : '';
    const link = commitLinkHtml(commit, 'unknown');
    el.innerHTML = pkgVersion
      ? `${link} <span class="muted">(v${escapeHtml(pkgVersion)})</span>`
      : link;
  }).catch((e) => {
    logError('about:fetchApiVersion', e);
    const el = document.getElementById('about-api-version');
    if (el) el.innerHTML = '<span class="mono">unavailable</span>';
  });
}

export function initAbout() {
  const btn = document.getElementById('about-dashboard-btn');
  if (!btn) return;
  btn.addEventListener('click', (e) => {
    e.stopPropagation();
    const wrap = document.getElementById('account-dropdown');
    const trigger = document.getElementById('account-dropdown-trigger');
    const panel = document.getElementById('account-dropdown-panel');
    if (wrap) wrap.classList.remove('is-open');
    if (trigger) trigger.setAttribute('aria-expanded', 'false');
    if (panel) panel.setAttribute('aria-hidden', 'true');
    openAboutModal();
  });
}
