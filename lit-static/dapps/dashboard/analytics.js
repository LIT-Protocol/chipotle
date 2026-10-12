import { createAnalytics } from '../../analytics.js';
import { getApiKey, getMode, getChainSecuredWallet, getClient, isAuthenticated } from './auth.js';

export const analytics = createAnalytics('dashboard');
const OPERATIONS = new Set(['newAccount', 'newChainSecuredAccount', 'createWallet',
  'deleteWallet', 'addGroup', 'removeGroup', 'updateGroup', 'addAction', 'deleteAction',
  'updateActionMetadata', 'addUsageApiKey', 'updateUsageApiKey', 'removeUsageApiKey', 'litAction']);

export async function trackDashboardOperation(operation, mode, run) {
  if (!OPERATIONS.has(operation)) return run();
  analytics.capture('product_operation_started', { operation, mode });
  try {
    const result = await run();
    // SDK resolution is a browser-observed completion, not an accounting event.
    analytics.capture('product_operation_completed', { operation, mode });
    return result;
  } catch (error) {
    analytics.capture('product_operation_failed', { operation, mode });
    throw error;
  }
}

let sessionKey;
export function endDashboardSession() {
  sessionKey = undefined;
  analytics.capture('signed_out');
  analytics.reset();
}
export async function trackDashboardSession() {
  try {
    if (!isAuthenticated()) return;
    const mode = getMode();
    // This value is only a local race guard; it is never sent to analytics.
    const credential = mode === 'sovereign' ? getChainSecuredWallet() : getApiKey();
    if (sessionKey === credential) return;
    if (sessionKey) analytics.reset();
    sessionKey = credential;
    const marker = JSON.parse(sessionStorage.getItem('chipotle_password_identity') || 'null');
    if (mode === 'api' && marker?.id) analytics.identify('dashboard-user:' + marker.id, { email: marker.email });
    // Wallet/API-key account access does not establish a human identity. Keep
    // those visitors anonymous and associate their activity with the account.
    analytics.capture('session_resumed', { method: marker?.id ? 'password' : mode });
    const client = await getClient();
    let wallet;
    if (mode === 'sovereign') wallet = await client.getBillingWalletAddress({ apiKey: '' });
    else {
      const wallets = await client.listWallets({ apiKey: credential, pageNumber: '0', pageSize: '20' });
      const master = wallets?.find((w) => {
        const label = String(w.description || w.name || '').toLowerCase();
        return label.includes('account master wallet') || label === 'amw';
      });
      wallet = master?.wallet_address || master?.address;
    }
    const current = getMode() === 'sovereign' ? getChainSecuredWallet() : getApiKey();
    if (current !== credential || !isAuthenticated()) return;
    if (/^0x[0-9a-fA-F]{40}$/.test(wallet)) analytics.group('dashboard:' + wallet.toLowerCase());
  } catch { /* No analytics failure should affect login or billing. */ }
}
