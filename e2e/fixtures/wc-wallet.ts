import { Core } from '@walletconnect/core';
import { WalletKit, type IWalletKit } from '@reown/walletkit';
import { getSdkError, buildApprovedNamespaces } from '@walletconnect/utils';
import {
  createWalletClient,
  http,
  hexToString,
  isHex,
  type Hex,
  type Address,
} from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import { foundry } from 'viem/chains';
import { ANVIL_RPC_URL, walletFor } from './anvil';

const WC_PROJECT_ID = process.env.WC_PROJECT_ID;
if (!WC_PROJECT_ID) {
  // We don't throw at import time — the WC projects/tests will fail loudly if
  // you forgot to set this. Unit tests / MM tests don't need it.
  console.warn('[wc-wallet] WC_PROJECT_ID is not set — WalletConnect tests will fail.');
}

const CHAIN_ID = `eip155:${foundry.id}`;

// Everything this headless wallet can actually do. A real wallet only ever
// approves a subset of what a dapp proposes.
const SUPPORTED_METHODS = [
  'eth_sendTransaction',
  'eth_signTransaction',
  'personal_sign',
  'eth_sign',
  'eth_signTypedData',
  'eth_signTypedData_v4',
];

export interface TestWcWallet {
  walletkit: IWalletKit;
  address: Address;
  pair(uri: string): Promise<void>;
  disconnectAll(): Promise<void>;
  /**
   * Every `session_request` method this wallet was actually asked to handle, in
   * order. Lets a test prove a signing request reached the WALLET rather than
   * being misrouted to the rpcMap HTTP node — critical because the local e2e
   * fallback RPC is Anvil, which would happily sign eth_signTypedData_v4 with
   * its unlocked deterministic accounts and mask a routing regression.
   */
  readonly handledMethods: string[];
}

export interface CreateTestWcWalletOpts {
  accountIndex?: number;
  /**
   * Mimic a restrictive real-world wallet: approve ONLY the methods the dapp
   * marks as *required*, dropping everything it proposed as optional. This is
   * the configuration that surfaces WalletConnect's RPC-routing trap — any
   * signing method left out of the approved namespace gets routed to the
   * rpcMap HTTP node instead of the wallet. With `eth_signTypedData_v4` only
   * optional, the dashboard's typed-data sign would hit the public RPC and get
   * `-32601`; declaring it required (the fix) keeps it in the namespace.
   */
  approveOnlyRequired?: boolean;
}

/**
 * Spin up an in-process WalletConnect v2 wallet. Auto-approves any incoming
 * session proposal for the configured Anvil account, and signs/sends any
 * eth_* requests with viem against Anvil.
 *
 * Use one instance per test for isolation.
 */
export async function createTestWcWallet(
  opts: CreateTestWcWalletOpts | number = {},
): Promise<TestWcWallet> {
  // Back-compat: older callers passed a bare accountIndex.
  const { accountIndex = 1, approveOnlyRequired = false } =
    typeof opts === 'number' ? { accountIndex: opts } : opts;
  const { account, privateKey } = walletFor(accountIndex);
  const signer = privateKeyToAccount(privateKey);

  const viemClient = createWalletClient({
    account: signer,
    chain: foundry,
    transport: http(ANVIL_RPC_URL),
  });

  const core = new Core({ projectId: WC_PROJECT_ID });
  const walletkit = await WalletKit.init({
    core,
    metadata: {
      name: 'e2e-test-wallet',
      description: 'Headless test wallet',
      url: 'http://localhost',
      icons: [],
    },
  });

  walletkit.on('session_proposal', async ({ id, params }) => {
    let approvedNamespaces;
    if (approveOnlyRequired) {
      // Hand-build the namespace from the dapp's *required* methods only, so
      // optional proposals (which buildApprovedNamespaces would auto-include)
      // are dropped — reproducing a wallet that doesn't echo optional methods.
      const requiredMethods = params.requiredNamespaces?.eip155?.methods ?? [];
      approvedNamespaces = {
        eip155: {
          chains: [CHAIN_ID],
          methods: requiredMethods.filter((m: string) => SUPPORTED_METHODS.includes(m)),
          events: ['accountsChanged', 'chainChanged'],
          accounts: [`${CHAIN_ID}:${account.address}`],
        },
      };
    } else {
      approvedNamespaces = buildApprovedNamespaces({
        proposal: params,
        supportedNamespaces: {
          eip155: {
            chains: [CHAIN_ID],
            methods: SUPPORTED_METHODS,
            events: ['accountsChanged', 'chainChanged'],
            accounts: [`${CHAIN_ID}:${account.address}`],
          },
        },
      });
    }

    await walletkit.approveSession({ id, namespaces: approvedNamespaces });
  });

  const handledMethods: string[] = [];

  walletkit.on('session_request', async ({ topic, params, id }) => {
    const { request } = params;
    let result: unknown;
    // Record the method BEFORE dispatch: a request that reaches this handler
    // was routed to the wallet (not the rpcMap HTTP node), which is exactly
    // what the routing regression test needs to assert.
    handledMethods.push(request.method);

    try {
      switch (request.method) {
        case 'personal_sign': {
          // params: [message, address]
          const [maybeHex] = request.params as [Hex | string, Address];
          const message = isHex(maybeHex) ? hexToString(maybeHex) : maybeHex;
          result = await viemClient.signMessage({ message });
          break;
        }
        case 'eth_signTypedData':
        case 'eth_signTypedData_v4': {
          // params: [address, typedDataJsonString]
          const [, typedDataStr] = request.params as [Address, string];
          const typed = JSON.parse(typedDataStr);
          result = await viemClient.signTypedData(typed);
          break;
        }
        case 'eth_sendTransaction': {
          const [tx] = request.params as [{
            from: Address;
            to: Address;
            data?: Hex;
            value?: Hex;
            gas?: Hex;
          }];
          result = await viemClient.sendTransaction({
            to: tx.to,
            data: tx.data,
            value: tx.value ? BigInt(tx.value) : undefined,
            gas: tx.gas ? BigInt(tx.gas) : undefined,
          });
          break;
        }
        default:
          throw new Error(`Unsupported method ${request.method}`);
      }

      await walletkit.respondSessionRequest({
        topic,
        response: { id, jsonrpc: '2.0', result },
      });
    } catch (err) {
      await walletkit.respondSessionRequest({
        topic,
        response: {
          id,
          jsonrpc: '2.0',
          error: { code: 5000, message: (err as Error).message },
        },
      });
    }
  });

  return {
    walletkit,
    address: account.address,
    handledMethods,
    async pair(uri: string) {
      await walletkit.pair({ uri });
    },
    async disconnectAll() {
      const sessions = walletkit.getActiveSessions();
      for (const topic of Object.keys(sessions)) {
        await walletkit.disconnectSession({
          topic,
          reason: getSdkError('USER_DISCONNECTED'),
        });
      }
    },
  };
}
