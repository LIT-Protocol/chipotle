import { task } from "hardhat/config";
import SafeApiKit from "@safe-global/api-kit";
import Safe from "@safe-global/protocol-kit";
import { ethers } from "ethers";
import { readFileSync } from "fs";

task("propose-diamond-cut", "Propose a diamondCut transaction through a Safe multisig")
  .addParam("safe", "Safe multisig address")
  .addParam("proposal", "Path to the diamond cut proposal JSON file")
  .setAction(async (taskArgs, hre) => {
    const proposerKey = process.env.PROPOSER_PRIVATE_KEY;
    if (!proposerKey) {
      throw new Error("PROPOSER_PRIVATE_KEY environment variable is required");
    }

    const { safe: safeAddress, proposal: proposalPath } = taskArgs;
    const network = hre.network.name;
    const chainId = hre.network.config.chainId;

    if (!chainId) {
      throw new Error(`Chain ID not configured for network ${network}`);
    }

    console.log(`Network: ${network} (chain ${chainId})`);
    console.log(`Safe: ${safeAddress}`);
    console.log(`Proposal file: ${proposalPath}`);

    // Read proposal JSON
    const proposalData = JSON.parse(readFileSync(proposalPath, "utf-8"));
    console.log(`\nTarget contract: ${proposalData.to}`);
    console.log(`Calldata length: ${proposalData.data.length} chars`);
    if (proposalData.facets_deployed) {
      console.log("\nDeployed facets:");
      for (const [name, addr] of Object.entries(proposalData.facets_deployed)) {
        console.log(`  ${name}: ${addr}`);
      }
    }

    // Initialize Protocol Kit with proposer signer
    const rpcUrl =
      (hre.network.config as { url?: string }).url || "https://mainnet.base.org";

    const protocolKit = await Safe.init({
      provider: rpcUrl,
      signer: proposerKey,
      safeAddress,
    });

    // Create the Safe transaction
    const safeTransaction = await protocolKit.createTransaction({
      transactions: [
        {
          to: proposalData.to,
          data: proposalData.data,
          value: proposalData.value || "0",
          operation: proposalData.operation ?? 0,
        },
      ],
    });

    // Sign the transaction with the proposer's key
    const signedTransaction = await protocolKit.signTransaction(safeTransaction);
    const safeTxHash = await protocolKit.getTransactionHash(signedTransaction);

    console.log(`\nSafe transaction hash: ${safeTxHash}`);

    // Submit to Safe Transaction Service
    const apiKit = new SafeApiKit({ chainId: BigInt(chainId) });

    const signerAddress = new ethers.Wallet(proposerKey).address;

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
    console.log(
      `\nOther signers can review and sign the transaction in the Safe UI.`
    );
    console.log(`Safe TX Hash: ${safeTxHash}`);
  });
