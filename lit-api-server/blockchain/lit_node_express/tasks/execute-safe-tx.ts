import { task } from "hardhat/config";
import SafeApiKit from "@safe-global/api-kit";

task("execute-safe-tx", "Verify a Safe transaction has been executed and return its on-chain tx hash")
  .addParam("safe", "Safe multisig address")
  .addParam("safeTxHash", "The Safe transaction hash to verify")
  .setAction(async (taskArgs, hre) => {
    const { safe: safeAddress, safeTxHash } = taskArgs;
    const chainId = hre.network.config.chainId;

    if (!chainId) {
      throw new Error(`Chain ID not configured for network ${hre.network.name}`);
    }

    console.log(`Network: ${hre.network.name} (chain ${chainId})`);
    console.log(`Safe: ${safeAddress}`);
    console.log(`Safe TX Hash: ${safeTxHash}`);

    const apiKit = new SafeApiKit({ chainId: BigInt(chainId) });

    const safeTransaction = await apiKit.getTransaction(safeTxHash);

    if (!safeTransaction.isExecuted || !safeTransaction.transactionHash) {
      console.error(
        `\nSafe transaction has not been executed yet. ` +
          `Please execute the transaction in the Safe UI before running this workflow.`
      );
      process.exit(1);
    }

    if (safeTransaction.isSuccessful === false) {
      console.error(
        `\nSafe transaction was executed but REVERTED on-chain (tx: ${safeTransaction.transactionHash}). ` +
        `The addComposeHash call did not succeed. Check the transaction on Basescan.`
      );
      process.exit(1);
    }

    console.log(`\nTransaction already executed on-chain.`);
    console.log(`Transaction hash: ${safeTransaction.transactionHash}`);
    // Machine-readable output for CI pipelines
    console.log(`TX_HASH=${safeTransaction.transactionHash}`);
  });
