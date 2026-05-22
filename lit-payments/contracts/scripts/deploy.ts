import { ethers, network, run } from "hardhat";
import { isAddress } from "ethers";

/// Deploy LitkeyPaymentGateway to whichever Hardhat network is active.
///
/// Required env vars:
///   LITKEY_ADDRESS     — ERC-20 token address on this chain
///   TREASURY_ADDRESS   — destination for swept LITKEY (e.g., company Safe)
///   DEPLOYER_PRIVATE_KEY — funded deployer wallet (set in hardhat.config.ts)
///
/// Optional:
///   BASESCAN_API_KEY   — if set, contract is verified after a 10s settle.
async function main() {
  const litkey = process.env.LITKEY_ADDRESS;
  const treasury = process.env.TREASURY_ADDRESS;

  if (!litkey || !isAddress(litkey)) {
    throw new Error(`LITKEY_ADDRESS missing or invalid: ${litkey ?? "<unset>"}`);
  }
  if (!treasury || !isAddress(treasury)) {
    throw new Error(
      `TREASURY_ADDRESS missing or invalid: ${treasury ?? "<unset>"}`,
    );
  }

  const [deployer] = await ethers.getSigners();
  const deployerAddr = await deployer.getAddress();
  const balance = await ethers.provider.getBalance(deployerAddr);

  console.log(`Network:  ${network.name} (chainId ${network.config.chainId})`);
  console.log(`Deployer: ${deployerAddr}`);
  console.log(`Balance:  ${ethers.formatEther(balance)} ETH`);
  console.log(`LITKEY:   ${litkey}`);
  console.log(`Treasury: ${treasury}`);
  console.log("");

  const Gateway = await ethers.getContractFactory("LitkeyPaymentGateway");
  const gateway = await Gateway.deploy(litkey, treasury);
  const tx = gateway.deploymentTransaction();
  console.log(`Deploy tx: ${tx?.hash}`);
  await gateway.waitForDeployment();
  const address = await gateway.getAddress();
  console.log(`Deployed:  ${address}`);

  if (process.env.BASESCAN_API_KEY) {
    console.log("Waiting 10s for Basescan to index the deployment...");
    await new Promise((r) => setTimeout(r, 10_000));
    try {
      await run("verify:verify", {
        address,
        constructorArguments: [litkey, treasury],
      });
      console.log("Verified on Basescan.");
    } catch (e) {
      console.warn(
        "Basescan verify failed (you can re-run `pnpm verify:base-sepolia <address> <litkey> <treasury>` later):",
        e instanceof Error ? e.message : e,
      );
    }
  } else {
    console.log(
      "BASESCAN_API_KEY not set — skipping verify. Run `pnpm verify:<network> <address> <litkey> <treasury>` manually if you want it.",
    );
  }
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
