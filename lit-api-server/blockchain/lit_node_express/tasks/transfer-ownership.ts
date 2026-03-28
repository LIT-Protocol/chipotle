import { task } from "hardhat/config";
import { ethers } from "ethers";

// Minimal ABI for OwnershipFacet
const OWNERSHIP_ABI = [
  "function transferOwnership(address _newOwner)",
  "function owner() view returns (address)",
];

task("transfer-ownership", "Transfer Diamond ownership to a Safe multisig (irreversible)")
  .addParam("diamond", "Diamond proxy contract address")
  .addParam("newOwner", "New owner address (Safe multisig)")
  .setAction(async (taskArgs, hre) => {
    const ownerKey = process.env.OWNER_PRIVATE_KEY;
    if (!ownerKey) {
      throw new Error("OWNER_PRIVATE_KEY environment variable is required");
    }

    const { diamond: diamondAddress, newOwner } = taskArgs;

    console.log(`Network: ${hre.network.name}`);
    console.log(`Diamond: ${diamondAddress}`);
    console.log(`New Owner: ${newOwner}`);

    const rpcUrl =
      (hre.network.config as { url?: string }).url || "https://mainnet.base.org";

    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const wallet = new ethers.Wallet(ownerKey, provider);
    const diamond = new ethers.Contract(diamondAddress, OWNERSHIP_ABI, wallet);

    // Check current owner
    const currentOwner = await diamond.owner();
    console.log(`\nCurrent owner: ${currentOwner}`);
    console.log(`Caller: ${wallet.address}`);

    if (currentOwner.toLowerCase() !== wallet.address.toLowerCase()) {
      throw new Error(
        `Caller ${wallet.address} is not the current owner ${currentOwner}`
      );
    }

    if (currentOwner.toLowerCase() === newOwner.toLowerCase()) {
      console.log("New owner is already the current owner. Nothing to do.");
      return;
    }

    console.log(
      `\n⚠️  WARNING: This is an irreversible operation!`
    );
    console.log(
      `Transferring ownership from ${currentOwner} to ${newOwner}...`
    );

    const tx = await diamond.transferOwnership(newOwner);
    console.log(`Transaction sent: ${tx.hash}`);

    const receipt = await tx.wait();
    console.log(`Transaction confirmed in block ${receipt.blockNumber}`);

    // Verify
    const updatedOwner = await diamond.owner();
    console.log(`\nVerification: new owner is ${updatedOwner}`);

    if (updatedOwner.toLowerCase() === newOwner.toLowerCase()) {
      console.log("Ownership transfer successful!");
    } else {
      throw new Error(
        `Ownership transfer verification failed! Expected ${newOwner}, got ${updatedOwner}`
      );
    }
  });
