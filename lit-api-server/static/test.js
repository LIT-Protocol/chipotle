/**
 * Simple generic test (no API calls). Use manual test sections in simple_api_test.html to exercise endpoints.
 * - Node: node test.js (API_BASE_URL env optional).
 * - Browser: load from simple_api_test.html; set window.LIT_SIMPLE_API_BASE_URL or use the page input.
 *
 * API endpoints (core_sdk.js / simple_api_test.html):
 * - new_account, create_wallet, lit_action, sign_with_pkp
 * - add_group (group_name, group_description, permitted_actions, pkps)
 * - add_action_to_group (api_key, group_id, action_ipfs_cid, name?, description?)
 * - add_pkp_to_group, remove_pkp_from_group
 * - add_usage_api_key, remove_usage_api_key
 * - list_groups, list_wallets, list_wallets_in_group, list_actions (GET, paginated: api_key, page_number, page_size; list_wallets_in_group and list_actions also require group_id)
 */

function getBaseUrl() {
  if (typeof process !== 'undefined' && process.env && process.env.API_BASE_URL) return process.env.API_BASE_URL;
  if (typeof window !== 'undefined' && window.LIT_SIMPLE_API_BASE_URL) return window.LIT_SIMPLE_API_BASE_URL;
  return 'http://localhost:8000';
}

/**
 * Run the generic test flow. Exported for use from index.html.
 * No API calls are made; use the manual test sections in the page to exercise endpoints.
 * @param {string} [baseUrl] - Override base URL (default: getBaseUrl())
 * @returns {Promise<void>}
 */
export async function runTests(baseUrl = getBaseUrl()) {
  console.log('Generic test (no API calls). Use the manual test sections below to exercise endpoints.');
  console.log('Done.');
}

// Run when executed directly (e.g. node test.js)
if (typeof window === 'undefined') {
  runTests().catch((err) => {
    console.error('Test failed:', err.message);
    process.exit(1);
  });
}
