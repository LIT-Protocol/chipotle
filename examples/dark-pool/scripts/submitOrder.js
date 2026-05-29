// Submit one encrypted order to the dark pool.
//
//   npm run submit -- --side buy  --price 100 --qty 5 --trader 0xabc...
//   npm run submit -- --side sell --price 100 --qty 5 --key 0xPRIVKEY
//
// Human price/qty are scaled to chain units here: quantity in base
// smallest-units (1e18), limitPrice in quote-per-base x1e18 — the units the
// match action and DarkPoolSettlement.sol expect.
//
// The order is sealed and stored entirely inside the encryptOrder action; this
// script only ships the (already-public) parameters to it. If contracts are
// deployed and a key is supplied, it also escrows the backing tokens so the
// order can actually settle.

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");
const { runAction } = require("./lit");

const ERC20_ABI = [
  "function mint(address to, uint256 amount)",
  "function approve(address spender, uint256 amount) returns (bool)",
];
const SETTLEMENT_ABI = [
  "function depositBase(uint256 amount)",
  "function depositQuote(uint256 amount)",
];

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : def;
}

async function main() {
  env.load();
  const base = process.env.LIT_API_BASE || "https://api.chipotle.litprotocol.com";
  const usageKey = process.env.LIT_USAGE_API_KEY;
  const pkpId = process.env.VAULT_PKP_ADDRESS;
  const encryptedDbUrl = process.env.ENCRYPTED_DATABASE_URL;
  const pair = process.env.PAIR || "BASE/QUOTE";
  for (const [k, v] of [
    ["LIT_USAGE_API_KEY", usageKey],
    ["VAULT_PKP_ADDRESS", pkpId],
    ["ENCRYPTED_DATABASE_URL", encryptedDbUrl],
  ]) {
    if (!v) throw new Error(`${k} missing — run \`npm run setup\` first`);
  }

  const side = arg("side");
  if (side !== "buy" && side !== "sell") throw new Error("--side must be buy or sell");
  const price = arg("price");
  const qty = arg("qty");
  if (!price || !qty) throw new Error("--price and --qty are required");
  const epoch = Number(arg("epoch", "1"));

  const key = arg("key", process.env.DEPLOYER_PRIVATE_KEY);
  let trader = arg("trader");
  let signer = null;
  if (key) {
    signer = new ethers.Wallet(key);
    trader = signer.address;
  }
  if (!trader) throw new Error("supply --trader <address> (confidential-only) or --key / DEPLOYER_PRIVATE_KEY");

  const limitPrice = ethers.utils.parseUnits(price, 18).toString();
  const quantity = ethers.utils.parseUnits(qty, 18).toString();

  // 1. Seal + store the order (encrypt + INSERT happen inside the action).
  const code = fs.readFileSync(path.join(__dirname, "..", "action", "encryptOrder.js"), "utf8");
  const res = await runAction(base, usageKey, code, {
    pkpId,
    encryptedDbUrl,
    epoch,
    pair,
    order: { side, limitPrice, quantity, trader },
  });
  if (!res || !res.ok) throw new Error(`encryptOrder rejected: ${JSON.stringify(res)}`);
  console.log(`sealed ${side} ${qty} @ ${price} for ${trader} -> order #${res.id} (epoch ${epoch})`);

  // 2. Escrow the backing tokens, if contracts are deployed and we hold a key.
  const settlement = process.env.SETTLEMENT_ADDRESS;
  if (settlement && signer) {
    const provider = new ethers.providers.JsonRpcProvider(process.env.RPC_URL);
    const w = signer.connect(provider);
    const isSell = side === "sell";
    const tokenAddr = isSell ? process.env.BASE_TOKEN_ADDRESS : process.env.QUOTE_TOKEN_ADDRESS;
    const amount = isSell
      ? ethers.BigNumber.from(quantity)
      : ethers.BigNumber.from(quantity).mul(limitPrice).div(ethers.constants.WeiPerEther);
    const token = new ethers.Contract(tokenAddr, ERC20_ABI, w);
    await (await token.mint(trader, amount)).wait();
    await (await token.approve(settlement, amount)).wait();
    const sc = new ethers.Contract(settlement, SETTLEMENT_ABI, w);
    await (await (isSell ? sc.depositBase(amount) : sc.depositQuote(amount))).wait();
    console.log(`escrowed ${ethers.utils.formatUnits(amount, 18)} ${isSell ? "base" : "quote"}`);
  } else if (!settlement) {
    console.log("(no SETTLEMENT_ADDRESS — confidential-only; deploy contracts to escrow + settle)");
  }
}

main().catch((err) => {
  console.error("submit failed:", err.message);
  if (err.body) console.error("server said:", err.body);
  process.exit(1);
});
