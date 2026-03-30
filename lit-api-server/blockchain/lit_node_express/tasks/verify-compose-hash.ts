import { task } from "hardhat/config";
import { ethers } from "ethers";

const DSTACK_APP_ABI = [
  "function allowedComposeHashes(bytes32) view returns (bool)",
];

task("verify-compose-hash", "Verify a compose hash is whitelisted on the DstackApp contract")
  .addParam("appAuth", "AppAuth (DstackApp) contract address")
  .addParam("composeHash", "The compose hash to verify (hex, 32 bytes)")
  .setAction(async (taskArgs, hre) => {
    const { appAuth: appAuthAddress, composeHash } = taskArgs;
    const chainId = hre.network.config.chainId;

    if (!chainId) {
      throw new Error(`Chain ID not configured for network ${hre.network.name}`);
    }

    console.log(`Network: ${hre.network.name} (chain ${chainId})`);
    console.log(`AppAuth: ${appAuthAddress}`);
    console.log(`Compose Hash: ${composeHash}`);

    const rpcUrl =
      (hre.network.config as { url?: string }).url || "https://mainnet.base.org";
    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const contract = new ethers.Contract(appAuthAddress, DSTACK_APP_ABI, provider);

    const composeHashBytes = composeHash.startsWith("0x") ? composeHash : `0x${composeHash}`;
    const isAllowed: boolean = await contract.allowedComposeHashes(composeHashBytes);

    console.log(`\nOn-chain result: allowedComposeHashes(${composeHashBytes}) = ${isAllowed}`);
    // Machine-readable output for CI pipelines
    console.log(`ALLOWED=${isAllowed}`);

    if (!isAllowed) {
      console.error(
        `\nCompose hash is NOT whitelisted on the DstackApp contract. ` +
        `The addComposeHash transaction may have reverted or not been executed.`
      );
      process.exit(1);
    }
  });
