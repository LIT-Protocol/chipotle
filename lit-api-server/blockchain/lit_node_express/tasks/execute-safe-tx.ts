import { task } from "hardhat/config";
import SafeApiKit from "@safe-global/api-kit";
import Safe from "@safe-global/protocol-kit";

task("execute-safe-tx", "Execute a Safe transaction after threshold signatures are collected")
  .addParam("safe", "Safe multisig address")
  .addParam("safeTxHash", "The Safe transaction hash to execute")
  .setAction(async (taskArgs, hre) => {
    const executorKey = process.env.EXECUTOR_PRIVATE_KEY;
    if (!executorKey) {
      throw new Error("EXECUTOR_PRIVATE_KEY environment variable is required");
    }

    const { safe: safeAddress, safeTxHash } = taskArgs;
    const chainId = hre.network.config.chainId;

    if (!chainId) {
      throw new Error(`Chain ID not configured for network ${hre.network.name}`);
    }

    console.log(`Network: ${hre.network.name} (chain ${chainId})`);
    console.log(`Safe: ${safeAddress}`);
    console.log(`Safe TX Hash: ${safeTxHash}`);

    const apiKit = new SafeApiKit({ chainId: BigInt(chainId) });

    // Fetch the transaction and check signatures
    const safeTransaction = await apiKit.getTransaction(safeTxHash);
    const threshold = (
      await apiKit.getSafeInfo(safeAddress)
    ).threshold;
    const confirmations = safeTransaction.confirmations?.length ?? 0;

    console.log(`\nThreshold: ${threshold}`);
    console.log(`Confirmations: ${confirmations}`);

    if (confirmations < threshold) {
      console.error(
        `\nInsufficient signatures: ${confirmations}/${threshold}. ` +
          `Need ${threshold - confirmations} more signature(s).`
      );
      process.exit(1);
    }

    console.log(`\nThreshold met. Executing transaction on-chain...`);

    const rpcUrl =
      (hre.network.config as { url?: string }).url || "https://mainnet.base.org";

    const protocolKit = await Safe.init({
      provider: rpcUrl,
      signer: executorKey,
      safeAddress,
    });

    const executeTxResponse = await protocolKit.executeTransaction(safeTransaction);

    console.log(`\nTransaction executed!`);
    console.log(`Transaction hash: ${executeTxResponse.hash}`);
  });
