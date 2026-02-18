/**
 * Simple test: get API key → create wallet → execute lit action.
 * - Node: node test.js (API_BASE_URL env optional). Ensure the simple API server is running.
 * - Browser: load from index.html; set window.LIT_SIMPLE_API_BASE_URL or use the page input.
 */

import { createClient } from './core_sdk.js';

function getBaseUrl() {
  if (typeof process !== 'undefined' && process.env && process.env.API_BASE_URL) return process.env.API_BASE_URL;
  if (typeof window !== 'undefined' && window.LIT_SIMPLE_API_BASE_URL) return window.LIT_SIMPLE_API_BASE_URL;
  return 'http://localhost:8000';
}

/**
 * Run the test flow. Exported for use from index.html.
 * @param {string} [baseUrl] - Override base URL (default: getBaseUrl())
 * @returns {Promise<void>}
 */
export async function runTests(baseUrl = getBaseUrl()) {
  const client = createClient(baseUrl);

  console.log('1. Getting API key...');
  const { api_key, wallet_address } = await client.getApiKey();
  console.log('   api_key:', api_key);
  console.log('   wallet_address:', wallet_address ?? '(none)');

  console.log('2. Creating wallet...');
  const { wallet_address: createdWalletAddress } = await client.createWallet(api_key);
  console.log('   wallet_address:', createdWalletAddress ?? '(none)');

  console.log('3. Executing lit action...');
  const litActionCode = `
    const go = async () => {
      Lit.Actions.setResponse({ response: JSON.stringify("Hello from lit action!") });
    };
    go();
  `;
  const litResult = await client.litAction({
    apiKey: api_key,
    code: litActionCode,
    jsParams: { testParam: 'hello' },
  });
  console.log('   response:', litResult?.response ?? '(none)');
  console.log('   logs (first 80 chars):', (litResult?.logs ?? '').slice(0, 80) + (litResult?.logs?.length > 80 ? '...' : ''));
  console.log('   has_error:', litResult?.has_error ?? '(none)');
  console.log('   signatures count:', litResult?.signatures?.length ?? 0);

  console.log('Done.');
}

// Run when executed directly (e.g. node test.js)
if (typeof window === 'undefined') {
  runTests().catch((err) => {
    console.error('Test failed:', err.message);
    process.exit(1);
  });
}
