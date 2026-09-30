/**
 * Listener-lag staleness banner.
 *
 * The API server keeps a per-instance cache on the action-execution path
 * (authorization decisions + wallet-derivation lookups), which an event listener
 * invalidates by polling the chain roughly every 10s (see
 * lit-api-server/src/account_events.rs). The dashboard's tables read the chain
 * live and are never stale; what lags is how quickly a *permission change* takes
 * effect when running an action. Behind a load balancer each instance has its
 * own cache, so after an on-chain write the server didn't originate — notably a
 * ChainSecured (sovereign) wallet write — a just-changed permission can take a
 * poll interval to apply on the instance running the action.
 *
 * `GET /core/v1/health` reports `account_event_listener_lag_seconds`. On a
 * healthy instance this stays at or below the poll interval; when it climbs, the
 * listener is stalled or dead and cache invalidation has fallen behind. We poll
 * that endpoint and show a banner once the lag crosses LAG_THRESHOLD_SECS.
 */
import { getBaseUrl } from './auth.js';
import { logError } from './ui-utils.js';

// Matches the >30s figure documented for users in management/account_modes.mdx
// and management/dashboard.mdx. Below this, brief lag is expected and not worth
// alarming about (it clears within a poll interval).
const LAG_THRESHOLD_SECS = 30;
const POLL_INTERVAL_MS = 30000;

let _timer = null;

async function fetchListenerLagSeconds() {
  const baseUrl = getBaseUrl().replace(/\/$/, '');
  const res = await fetch(`${baseUrl}/core/v1/health`, { method: 'GET' });
  // /core/v1/health returns 503 when the node is unhealthy, but the JSON body —
  // and the lag field — are still present in that case, so don't bail on !res.ok.
  const body = await res.json();
  const lag = body?.account_event_listener_lag_seconds;
  return typeof lag === 'number' ? lag : null;
}

function renderBanner(lagSeconds) {
  const banner = document.getElementById('listener-lag-banner');
  if (!banner) return;
  if (lagSeconds != null && lagSeconds > LAG_THRESHOLD_SECS) {
    banner.textContent =
      `This server is catching up to recent on-chain changes ` +
      `(account-event listener is ~${lagSeconds}s behind). The tables below are ` +
      `accurate, but a just-changed permission may take a moment to apply when ` +
      `you run an action. It resolves automatically once the server catches up.`;
    banner.style.display = '';
  } else {
    banner.style.display = 'none';
  }
}

async function checkOnce() {
  try {
    renderBanner(await fetchListenerLagSeconds());
  } catch (e) {
    // Network/parse errors are non-fatal — leave the banner as-is rather than
    // flapping it on a transient blip. A consistently failing health endpoint is
    // the load balancer's concern to surface, not a cache-staleness signal.
    logError('listener-lag', e);
  }
}

/**
 * Begin polling the health endpoint and toggling the staleness banner.
 * Idempotent — repeated calls (e.g. on every auth-ready) run an immediate check
 * but never stack interval timers.
 */
export function startListenerLagMonitor() {
  checkOnce();
  if (_timer != null) return;
  _timer = setInterval(checkOnce, POLL_INTERVAL_MS);
}
