import { task } from "hardhat/config";
import SafeApiKit from "@safe-global/api-kit";
import Safe from "@safe-global/protocol-kit";
import { ethers } from "ethers";

// Minimal ABI for DstackApp compose-hash whitelisting
const DSTACK_APP_ABI = [
  "function setComposeHash(bytes32 composeHash)",
  "function owner() view returns (address)",
];

task("propose-dstack-app", "Propose a DstackApp compose-hash whitelisting through Safe")
  .addParam("safe", "Safe multisig address")
  .addParam("dstackApp", "DstackApp contract address")
  .addParam("composeHash", "The compose hash to whitelist")
  .setAction(async (taskArgs, hre) => {
    const proposerKey = process.env.PROPOSER_PRIVATE_KEY;
    if (!proposerKey) {
      throw new Error("PROPOSER_PRIVATE_KEY environment variable is required");
    }

    const { safe: safeAddress, dstackApp: dstackAppAddress, composeHash } = taskArgs;
    const chainId = hre.network.config.chainId;

    if (!chainId) {
      throw new Error(`Chain ID not configured for network ${hre.network.name}`);
    }

    console.log(`Network: ${hre.network.name} (chain ${chainId})`);
    console.log(`Safe: ${safeAddress}`);
    console.log(`DstackApp: ${dstackAppAddress}`);
    console.log(`Compose Hash: ${composeHash}`);

    // Encode the setComposeHash call
    const iface = new ethers.Interface(DSTACK_APP_ABI);
    const calldata = iface.encodeFunctionData("setComposeHash", [composeHash]);

    console.log(`\nEncoded calldata: ${calldata}`);

    // Initialize Protocol Kit
    const rpcUrl =
      (hre.network.config as { url?: string }).url || "https://mainnet.base.org";

    const protocolKit = await Safe.init({
      provider: rpcUrl,
      signer: proposerKey,
      safeAddress,
    });

    // Create Safe transaction
    const safeTransaction = await protocolKit.createTransaction({
      transactions: [
        {
          to: dstackAppAddress,
          data: calldata,
          value: "0",
          operation: 0, // Call
        },
      ],
    });

    // Sign and propose
    const signedTransaction = await protocolKit.signTransaction(safeTransaction);
    const safeTxHash = await protocolKit.getTransactionHash(signedTransaction);

    console.log(`\nSafe transaction hash: ${safeTxHash}`);

    const apiKit = new SafeApiKit({ chainId: BigInt(chainId) });

    const signerAddress = await protocolKit.getAddress();

    await apiKit.proposeTransaction({
      safeAddress,
      safeTransactionData: signedTransaction.data,
      safeTxHash,
      senderAddress: signerAddress,
      senderSignature: signedTransaction.encodedSignatures(),
    });

    console.log(`\nTransaction proposed to Safe Transaction Service.`);
    console.log(
      `\nSafe UI: https://app.safe.global/transactions/queue?safe=base:${safeAddress}`
    );
    console.log(`Safe TX Hash: ${safeTxHash}`);
  });
