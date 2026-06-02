import { task } from "hardhat/config";
import SafeApiKit from "@safe-global/api-kit";
import Safe from "@safe-global/protocol-kit";
import { ethers } from "ethers";

// AppAuth (DstackApp) contract ABI for device allowlisting. Each Phala CVM
// with on-chain KMS has its own AppAuth contract; the KMS only releases keys
// to allowlisted device IDs. addDevice is onlyOwner — for Safe-owned apps
// (e.g. chipotle-prod) the call must go through the Safe, which is what this
// task proposes. Mirrors propose-dstack-app.ts (addComposeHash).
const APP_AUTH_ABI = [
  "function addDevice(bytes32 deviceId)",
  "function allowedDeviceIds(bytes32) view returns (bool)",
  "function owner() view returns (address)",
];

task("propose-add-device", "Propose an AppAuth device allowlisting through Safe")
  .addParam("safe", "Safe multisig address (the AppAuth owner)")
  .addParam("appAuth", "AppAuth contract address (per-app dstack_app_address / app_id, NOT the KMS factory)")
  .addParam("deviceId", "The Phala device ID to allowlist (hex, 32 bytes)")
  .setAction(async (taskArgs, hre) => {
    const proposerKey = process.env.PROPOSER_PRIVATE_KEY;
    if (!proposerKey) {
      throw new Error("PROPOSER_PRIVATE_KEY environment variable is required");
    }

    const { safe: safeAddress, appAuth: appAuthAddress, deviceId } = taskArgs;
    const chainId = hre.network.config.chainId;

    if (!chainId) {
      throw new Error(`Chain ID not configured for network ${hre.network.name}`);
    }

    const proposerWallet = new ethers.Wallet(proposerKey);
    const proposerAddress = proposerWallet.address;
    console.log(`Network: ${hre.network.name} (chain ${chainId})`);
    console.log(`Safe: ${safeAddress}`);
    console.log(`AppAuth: ${appAuthAddress}`);
    console.log(`Device ID: ${deviceId}`);
    console.log(`Proposer address: ${proposerAddress}`);

    // Encode the addDevice call
    const iface = new ethers.Interface(APP_AUTH_ABI);
    const deviceIdBytes = deviceId.startsWith("0x") ? deviceId : `0x${deviceId}`;
    const calldata = iface.encodeFunctionData("addDevice", [deviceIdBytes]);

    console.log(`\nEncoded calldata: ${calldata}`);
    // Raw values for manual entry into the Safe UI (Transaction Builder) if needed:
    console.log(`Manual entry — to: ${appAuthAddress}, value: 0, data: ${calldata}`);

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
