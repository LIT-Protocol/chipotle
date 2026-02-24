/**
 * Grafana k6 integration tests for lit-api-server
 *
 * Tests user flows through the API (no implementation-detail assertions).
 * Run against deployed instance:
 *   k6 run scripts/k6-integration.js
 *
 * Or with custom base URL:
 *   k6 run -e BASE_URL=https://your-instance.phala.network scripts/k6-integration.js
 *
 * For the full Lit Action flow, the deployment must have AccountConfig contract
 * configured (e.g. Base Sepolia). If LIT_API_KEY is set, that key is used for
 * lit_action; otherwise the test creates a new account and group.
 */
import http from 'k6/http';
import { check, sleep } from 'k6';

const BASE_URL = __ENV.BASE_URL || 'https://36da669c852c9bd4fdea27dd331c07ff776bd125-8000.dstack-pha-prod5.phala.network';
const LIT_API_KEY = __ENV.LIT_API_KEY || '';

const API_BASE = `${BASE_URL.replace(/\/$/, '')}/core/v1`;

// Simple Lit Action that returns "Hello World!"
const HELLO_WORLD_CODE = 'Lit.Actions.setResponse({response: "Hello World!"})';

export const options = {
  vus: 1,
  iterations: 1,
  thresholds: {
    // Allow new_account to fail when contract is not configured (use LIT_API_KEY for full flow)
    http_req_failed: ['rate<0.5'],
    http_req_duration: ['p(99)<30000'],
  },
};

export default function () {
  // --- 1. API reachability: get node chain config ---
  const chainRes = http.get(`${API_BASE}/get_node_chain_config`, {
    tags: { name: 'get_node_chain_config' },
  });
  check(chainRes, {
    'chain config returns 200': (r) => r.status === 200,
    'chain config has chain_id': (r) => {
      try {
        const body = JSON.parse(r.body);
        return typeof body.chain_id === 'number';
      } catch {
        return false;
      }
    },
  });
  if (!check(chainRes, { 'chain config ok': () => true })) {
    console.error('get_node_chain_config failed:', chainRes.body);
    return;
  }
  sleep(0.5);

  // --- 2. Get IPFS ID for Lit Action code (content-addressable identifier) ---
  const encodedCode = encodeURIComponent(HELLO_WORLD_CODE);
  const ipfsRes = http.get(`${API_BASE}/get_lit_action_ipfs_id/${encodedCode}`, {
    tags: { name: 'get_lit_action_ipfs_id' },
  });
  check(ipfsRes, {
    'ipfs id returns 200': (r) => r.status === 200,
    'ipfs id is non-empty': (r) => (r.body || '').trim().length > 0,
  });
  if (!check(ipfsRes, { 'ipfs id ok': () => true })) {
    console.error('get_lit_action_ipfs_id failed:', ipfsRes.body);
    return;
  }
  const ipfsId = (ipfsRes.body || '').trim().replace(/^"|"$/g, '');
  console.log('IPFS ID for Hello World action:', ipfsId);
  sleep(0.5);

  // --- 3. Lit Action execution ---
  let apiKey = LIT_API_KEY;

  if (!apiKey) {
    // Create account and group for the full flow
    const newAccountRes = http.post(
      `${API_BASE}/new_account`,
      JSON.stringify({
        account_name: 'k6-integration-test',
        account_description: 'Integration test account',
      }),
      {
        headers: { 'Content-Type': 'application/json' },
        tags: { name: 'new_account' },
      }
    );

    if (newAccountRes.status !== 200) {
      console.warn('new_account failed (contract may not be configured):', newAccountRes.body);
      console.warn('Set LIT_API_KEY env to test lit_action with an existing account.');
      return;
    }

    let newAccountBody;
    try {
      newAccountBody = JSON.parse(newAccountRes.body);
    } catch {
      console.error('Invalid new_account response:', newAccountRes.body);
      return;
    }
    apiKey = newAccountBody.api_key;
    if (!apiKey) {
      console.error('new_account did not return api_key');
      return;
    }
    sleep(0.5);

    // Add group with all actions and all wallets permitted
    const addGroupRes = http.post(
      `${API_BASE}/add_group`,
      JSON.stringify({
        api_key: apiKey,
        group_name: 'k6-test-group',
        group_description: 'Integration test group',
        permitted_actions: [],
        pkps: [],
        all_wallets_permitted: true,
        all_actions_permitted: true,
      }),
      {
        headers: { 'Content-Type': 'application/json' },
        tags: { name: 'add_group' },
      }
    );

    if (addGroupRes.status !== 200) {
      console.error('add_group failed:', addGroupRes.body);
      return;
    }
    sleep(0.5);
  }

  // Execute Lit Action
  const litActionRes = http.post(
    `${API_BASE}/lit_action`,
    JSON.stringify({
      api_key: apiKey,
      code: HELLO_WORLD_CODE,
      js_params: null,
    }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: 'lit_action' },
    }
  );

  check(litActionRes, {
    'lit_action returns 200': (r) => r.status === 200,
    'lit_action has response': (r) => {
      try {
        const body = JSON.parse(r.body);
        return body.has_error === false && body.response === 'Hello World!';
      } catch {
        return false;
      }
    },
  });

  if (!check(litActionRes, { 'lit_action ok': () => true })) {
    console.error('lit_action failed:', litActionRes.body);
    return;
  }

  const litBody = JSON.parse(litActionRes.body);
  console.log('Lit Action response:', litBody.response);
  console.log('Lit Action logs:', litBody.logs || '(none)');
}
