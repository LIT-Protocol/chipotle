import { task } from "hardhat/config";
import SafeApiKit from "@safe-global/api-kit";
import Safe from "@safe-global/protocol-kit";
import { ethers } from "ethers";

// AppAuth contract ABI for compose-hash whitelisting (from @phala/cloud SDK).
// Each Phala CVM with on-chain KMS has its own AppAuth contract (the dstack_app_address).
// NOTE: The KMS factory at 0x2f83172A49584C017F2B256F0FB2Dca14126Ba9C is NOT the AppAuth —
// pass the per-app dstack_app_address (visible via `phala cvms get <name> --json` → kms_info.dstack_app_address).
const APP_AUTH_ABI = [
  "function addComposeHash(bytes32 composeHash)",
  "function owner() view returns (address)",
];

task("propose-dstack-app", "Propose an AppAuth compose-hash whitelisting through Safe")
  .addParam("safe", "Safe multisig address")
  .addParam("appAuth", "AppAuth contract address (per-app dstack_app_address, NOT the KMS factory)")
  .addParam("composeHash", "The compose hash to whitelist (hex, 32 bytes)")
  .setAction(async (taskArgs, hre) => {
    const proposerKey = process.env.PROPOSER_PRIVATE_KEY;
    if (!proposerKey) {
      throw new Error("PROPOSER_PRIVATE_KEY environment variable is required");
    }

    const { safe: safeAddress, appAuth: appAuthAddress, composeHash } = taskArgs;
    const chainId = hre.network.config.chainId;

    if (!chainId) {
      throw new Error(`Chain ID not configured for network ${hre.network.name}`);
    }

    console.log(`Network: ${hre.network.name} (chain ${chainId})`);
    console.log(`Safe: ${safeAddress}`);
    console.log(`AppAuth: ${appAuthAddress}`);
    console.log(`Compose Hash: ${composeHash}`);

    // Encode the addComposeHash call
    const iface = new ethers.Interface(APP_AUTH_ABI);
    const composeHashBytes = composeHash.startsWith("0x") ? composeHash : `0x${composeHash}`;
    const calldata = iface.encodeFunctionData("addComposeHash", [composeHashBytes]);

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
          to: appAuthAddress,
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
