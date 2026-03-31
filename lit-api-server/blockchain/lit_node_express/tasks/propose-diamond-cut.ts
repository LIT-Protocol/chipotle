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

    // Create the Safe transaction (checksum the target address — the Rust
    // deployer outputs lowercase hex which the Safe API rejects).
    const safeTransaction = await protocolKit.createTransaction({
      transactions: [
        {
          to: ethers.getAddress(proposalData.to),
          data: proposalData.data,
          value: proposalData.value || "0",
          operation: proposalData.operation ?? 0,
        },
      ],
    });

    const safeTxHash = await protocolKit.getTransactionHash(safeTransaction);
    console.log(`\nSafe transaction hash: ${safeTxHash}`);

    // Check if proposer is an owner. If not, sign as delegate using eth_sign.
    const proposerWallet = new ethers.Wallet(proposerKey);
    const proposerAddress = proposerWallet.address;
    const owners = await protocolKit.getOwners();
    const isOwner = owners.some(
      (o) => o.toLowerCase() === proposerAddress.toLowerCase()
    );

    let senderSignature: string;

    if (isOwner) {
      const signed = await protocolKit.signTransaction(safeTransaction);
      senderSignature = signed.encodedSignatures();
    } else {
      console.log(`\nProposer is not an owner — signing as delegate (eth_sign)`);
      const messageBytes = ethers.getBytes(safeTxHash);
      const rawSig = proposerWallet.signingKey.sign(
        ethers.hashMessage(messageBytes)
      );
      // Safe expects v to be 31 or 32 for eth_sign signatures (v + 4)
      const v = rawSig.v - 27 + 31;
      senderSignature = ethers.solidityPacked(
        ["bytes32", "bytes32", "uint8"],
        [rawSig.r, rawSig.s, v]
      );
    }

    // Submit to Safe Transaction Service via direct HTTP (the SDK swallows
    // error response bodies, making 422 errors impossible to debug).
    console.log(`\nProposer address: ${proposerAddress}`);
    console.log(`Is owner: ${isOwner}`);

    const txServiceUrl = `https://safe-transaction-base.safe.global/api/v1/safes/${safeAddress}/multisig-transactions/`;
    const body = {
      ...safeTransaction.data,
      contractTransactionHash: safeTxHash,
      sender: proposerAddress,
      signature: senderSignature,
    };

    console.log(`\nPOST ${txServiceUrl}`);
    console.log(`Request nonce: ${body.nonce}`);

    const response = await fetch(txServiceUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });

    if (!response.ok) {
      const errorBody = await response.text();
      console.error(`\nSafe API error ${response.status}: ${errorBody}`);
      throw new Error(`Safe Transaction Service returned ${response.status}: ${errorBody}`);
    }

    console.log(`\nTransaction proposed to Safe Transaction Service.`);
    console.log(
      `\nSafe UI: https://app.safe.global/transactions/queue?safe=base:${safeAddress}`
    );
    console.log(`Safe TX Hash: ${safeTxHash}`);
    // Machine-readable output for CI pipelines
    console.log(`SAFE_TX_HASH=${safeTxHash}`);
  });
