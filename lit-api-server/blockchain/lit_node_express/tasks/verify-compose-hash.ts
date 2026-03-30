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
        `\nCompose hash is NOT whitelisted on DstackApp at ${appAuthAddress}. ` +
        `The CVM will reject this deployment with "Compose hash not allowed". ` +
        `Verify the Safe transaction targeted the correct contract (CVM app_id, not kms_info.dstack_app_address).`
      );
      process.exit(1);
    }
  });
