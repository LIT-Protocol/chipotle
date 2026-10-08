// Fund the oracle PKP with gas on both chains so it can broadcast mints in
// relay mode. Usage: node fundPkp.js [amountEth]  (default 0.005 per chain).
// Skips a chain if the oracle already holds >= amount.

const { ethers } = require("ethers");
const env = require("./_env");

async function main() {
  env.load();
  if (!process.env.ORACLE_ADDRESS) throw new Error("ORACLE_ADDRESS missing — run setup first");
  const oracle = process.env.ORACLE_ADDRESS;
  const amount = ethers.utils.parseEther(process.argv[2] || "0.005");

  const A = process.env.ALCHEMY_API_KEY;
  const chains = [
    { name: "Base", rpc: `https://base-mainnet.g.alchemy.com/v2/${A}` },
    { name: "Arbitrum", rpc: `https://arb-mainnet.g.alchemy.com/v2/${A}` },
  ];

  for (const c of chains) {
    const provider = new ethers.providers.JsonRpcProvider(c.rpc);
    const signer = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, provider);
    const bal = await provider.getBalance(oracle);
    if (bal.gte(amount)) {
      console.log(`${c.name}: oracle has ${ethers.utils.formatEther(bal)} ETH — skip`);
      continue;
    }
    const tx = await signer.sendTransaction({ to: oracle, value: amount });
    await tx.wait();
    console.log(`${c.name}: funded oracle ${oracle} with ${ethers.utils.formatEther(amount)} ETH -> ${tx.hash}`);
  }
}

main().catch((e) => {
  console.error("fundPkp failed:", e.message);
  process.exit(1);
});
