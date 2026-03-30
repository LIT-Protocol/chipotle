import { task } from "hardhat/config";
import SafeApiKit from "@safe-global/api-kit";
import Safe from "@safe-global/protocol-kit";
import { ethers } from "ethers";

// AppAuth contract ABI for compose-hash whitelisting (from @phala/cloud SDK).
// Each Phala CVM with on-chain KMS has its own AppAuth (DstackApp) contract.
// The correct address comes from kms_info.dstack_app_address (matches the CVM's
// app_id with 0x prefix).
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

    const proposerWallet = new ethers.Wallet(proposerKey);
    const proposerAddress = proposerWallet.address;
    console.log(`Network: ${hre.network.name} (chain ${chainId})`);
    console.log(`Safe: ${safeAddress}`);
    console.log(`AppAuth: ${appAuthAddress}`);
    console.log(`Compose Hash: ${composeHash}`);
    console.log(`Proposer address: ${proposerAddress}`);

    // Encode the addComposeHash call
    const iface = new ethers.Interface(APP_AUTH_ABI);
    const composeHashBytes = composeHash.startsWith("0x") ? composeHash : `0x${composeHash}`;
    const calldata = iface.encodeFunctionData("addComposeHash", [composeHashBytes]);

    console.log(`\nEncoded calldata: ${calldata}`);

    // Initialize Protocol Kit (use any valid signer — we only need it to build the tx)
    const rpcUrl =
      (hre.network.config as { url?: string }).url || "https://mainnet.base.org";

    const protocolKit = await Safe.init({
      provider: rpcUrl,
      signer: proposerKey,
      safeAddress,
    });

    // Create Safe transaction and compute its hash
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

    const safeTxHash = await protocolKit.getTransactionHash(safeTransaction);
    console.log(`\nSafe transaction hash: ${safeTxHash}`);

    // Check if proposer is an owner. If not, sign as delegate using eth_sign
    // (prepend "\x19Ethereum Signed Message:\n32" prefix).
    const owners = await protocolKit.getOwners();
    const isOwner = owners.some(
      (o) => o.toLowerCase() === proposerAddress.toLowerCase()
    );

    let senderSignature: string;

    if (isOwner) {
      // Owner: use Protocol Kit's EIP-712 typed-data signature
      const signed = await protocolKit.signTransaction(safeTransaction);
      senderSignature = signed.encodedSignatures();
    } else {
      // Delegate: produce an eth_sign signature (pre-image hashed, v adjusted)
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

    const apiKit = new SafeApiKit({ chainId: BigInt(chainId) });

    await apiKit.proposeTransaction({
      safeAddress,
      safeTransactionData: safeTransaction.data,
      safeTxHash,
      senderAddress: proposerAddress,
      senderSignature,
    });

    console.log(`\nTransaction proposed to Safe Transaction Service.`);
    console.log(
      `\nSafe UI: https://app.safe.global/transactions/queue?safe=base:${safeAddress}`
    );
    console.log(`Safe TX Hash: ${safeTxHash}`);
    // Machine-readable output for CI pipelines
    console.log(`SAFE_TX_HASH=${safeTxHash}`);
  });
