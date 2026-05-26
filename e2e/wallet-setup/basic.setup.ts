import { defineWalletSetup } from '@synthetixio/synpress';
import { MetaMask } from '@synthetixio/synpress/playwright';

// Anvil's default mnemonic. Synpress imports account #0 by default — that's
// the same account local_test.sh uses as deployer, so tests that touch chain
// state should be careful (snapshot/revert via the `anvilSnap` fixture
// handles this). To use a fresh user wallet instead, switch inside the test
// with `metamask.switchAccount(...)`.
const ANVIL_MNEMONIC =
  'test test test test test test test test test test test junk';

export const WALLET_PASSWORD = 'TestPassword!23';

// IMPORTANT: changing the contents of this function busts the Synpress wallet
// cache, which is what you want when you change network config or seed.
export default defineWalletSetup(WALLET_PASSWORD, async (context, walletPage) => {
  const metamask = new MetaMask(context, walletPage, WALLET_PASSWORD);

  await metamask.importWallet(ANVIL_MNEMONIC);

  await metamask.addNetwork({
    name: 'Anvil',
    rpcUrl: process.env.ANVIL_RPC_URL ?? 'http://127.0.0.1:8545',
    chainId: 31337,
    symbol: 'ETH',
    blockExplorerUrl: undefined,
  });

  // Switch the just-added network on so tests don't have to.
  await metamask.switchNetwork('Anvil');
});
