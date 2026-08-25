// Burn only — to test the LIVE lit-triggers relayer. After this, the triggers
// poller should observe the BurnInitiated event and auto-mint on the other
// chain (watch with: node watchRuns.js or the /api/triggers/<id>/runs endpoint).
//
// Usage: node burn.js [--from base] [--to arbitrum] [--amount 5] [--recipient 0x..]

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");

function loadAbi(name) {
  const p = path.join(__dirname, "..", "contracts", "out", `${name}.sol`, `${name}.json`);
  return JSON.parse(fs.readFileSync(p, "utf8")).abi;
}
function arg(flag, def) {
  const i = process.argv.indexOf(flag);
  return i !== -1 ? process.argv[i + 1] : def;
}

const CHAINS = {
  base: { id: 8453, name: "Base", tokenEnv: "BRIDGE_TOKEN_BASE_MAINNET", alchemySub: "base-mainnet" },
  arbitrum: { id: 42161, name: "Arbitrum", tokenEnv: "BRIDGE_TOKEN_ARB_MAINNET", alchemySub: "arb-mainnet" },
};

async function main() {
  env.load();
  const src = CHAINS[arg("--from", "base")];
  const dst = CHAINS[arg("--to", "arbitrum")];
  const amount = ethers.utils.parseUnits(arg("--amount", "5"), 18);
  const A = process.env.ALCHEMY_API_KEY;
  const provider = new ethers.providers.JsonRpcProvider(`https://${src.alchemySub}.g.alchemy.com/v2/${A}`);
  const dstProvider = new ethers.providers.JsonRpcProvider(`https://${dst.alchemySub}.g.alchemy.com/v2/${A}`);
  const signer = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY, provider);
  const recipient = arg("--recipient", await signer.getAddress());
  const token = new ethers.Contract(process.env[src.tokenEnv], loadAbi("BridgeToken"), signer);

  // Prepay the relayer's destination gas (native). 2x buffer over the action's
  // quote (gasPrice * 250k) to absorb gas drift between burn and mint.
  const prepay = (await dstProvider.getGasPrice()).mul(300000).mul(2); // covers action MINT_GAS_LIMIT (300k) + buffer
  console.log(`Burning ${arg("--amount", "5")} on ${src.name} -> ${dst.name} (recipient ${recipient}); gas prepay ${ethers.utils.formatEther(prepay)} ETH...`);
  const rcpt = await (await token.burn(amount, dst.id, recipient, { value: prepay })).wait();
  const topic = ethers.utils.id("BurnInitiated(address,address,uint256,uint256,uint256,uint256)");
  const log = rcpt.logs.find((l) => l.address.toLowerCase() === token.address.toLowerCase() && (l.topics[0] || "").toLowerCase() === topic.toLowerCase());
  console.log(`  burn tx ${rcpt.transactionHash} (block ${rcpt.blockNumber}, log ${log.logIndex})`);
  console.log(`\nNow the lit-triggers poller should auto-mint on ${dst.name} within ~1 min`);
  console.log(`(after CHAIN_CONFIRMATION_DEPTH blocks + poll interval). Watch the trigger runs.`);
}

main().catch((e) => { console.error("burn failed:", e.message); process.exit(1); });
