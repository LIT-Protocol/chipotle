/**
 * Simple test: create API key → mint PKP → sign "hello world" with that PKP → execute lit action.
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

  console.log('2. Minting PKP...');
  const { pkp_public_key } = await client.mintPkp(api_key);
  console.log('   pkp_public_key:', pkp_public_key);

  console.log('3. Signing "hello world" with PKP...');
  const signResp = await client.signWithPkp({
    apiKey: api_key,
    pkpPublicKey: pkp_public_key,
    message: 'hello world',
  });
  const { shares: signShares, signing_scheme } = signResp;
  console.log('   shares count:', signShares?.length ?? 0);
  console.log('   signing_scheme:', signing_scheme ?? '(none)');

  console.log('4. Executing lit action...');
  const litActionCode = `
    const go = async () => {
      Lit.Actions.setResponse({ response: JSON.stringify("Hello from lit action!") });
    };
    go();
  `;
  const { responses: litResponses } = await client.litAction({
    apiKey: api_key,
    code: litActionCode,
    jsParams: { testParam: 'hello' },
  });
  console.log('   responses count:', litResponses?.length ?? 0);
  if (litResponses?.length) {
    const first = litResponses[0];
    console.log('   first response.response:', first?.response ?? '(none)');
    console.log('   first response.logs (first 80 chars):', (first?.logs ?? '').slice(0, 80) + '...');
  }

  console.log('5. Encrypt...');
  const plaintext = 'secret message for encryption test';
  const { ciphertext, data_to_encrypt_hash } = await client.encrypt({ apiKey: api_key, message: plaintext });
  console.log('   ciphertext (first 60 chars):', (ciphertext || '').slice(0, 60) + '...');
  console.log('   data_to_encrypt_hash:', data_to_encrypt_hash?.slice(0, 24) + '...');

  console.log('6. Decrypt (using ciphertext and data_to_encrypt_hash from encrypt)...');
  const { decrypted_text } = await client.decrypt({
    apiKey: api_key,
    ciphertext,
    dataToEncryptHash: data_to_encrypt_hash,
  });
  console.log('   decrypted_text:', decrypted_text ?? '(none)');
  if (decrypted_text !== plaintext) {
    throw new Error(`Decrypt mismatch: expected "${plaintext}", got "${decrypted_text}"`);
  }
  console.log('   (decrypted text matches original plaintext)');

  console.log('7. Combine signature shares...');
  const shares = (signShares || []).filter((s) => s && (typeof s === 'object'));
  if (shares.length > 0) {
    const { signature, recovery_id } = await client.combineSignatureShares({
      apiKey: api_key,
      shareData: signResp,
    });
    console.log('   signature (first 40 chars):', (signature || '').slice(0, 40) + '...');
    console.log('   recovery_id:', recovery_id);
  } else {
    console.log('   (no signature shares; skipping combine)');
  }

  console.log('Done.');
}

// Run when executed directly (e.g. node test.js)
if (typeof window === 'undefined') {
  runTests().catch((err) => {
    console.error('Test failed:', err.message);
    process.exit(1);
  });
}
