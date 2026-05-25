import { defineWalletSetup } from '@synthetixio/synpress';
import { MetaMask } from '@synthetixio/synpress/playwright';

// Anvil's default mnemonic. Account #0 is the deployer; account #1 is what we
// use as the user wallet in tests so deploy-side state stays clean.
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
