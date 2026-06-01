// Submit one trader-signed, escrowed order to the dark pool.
//
//   npm run submit -- --side buy  --price 100 --qty 5 --epoch 1            (uses DEPLOYER_PRIVATE_KEY)
//   npm run submit -- --side sell --price 100 --qty 5 --epoch 1 --key 0x...
//
// The trader signs the order with their own key; matchEpoch verifies that
// signature inside the enclave before the order can be matched, so nobody (not
// even the operator holding the usage key) can forge an order for another
// address. The signing key is also the address whose escrow backs the order.
//
// Human price/qty are scaled to chain units: quantity in base smallest-units
// (1e18), limitPrice in quote-per-base x1e18 — the units matchEpoch and
// DarkPoolSettlement.sol expect. Requires deployed contracts (a signed order is
// bound to the settlement address + chain id).

const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { ethers } = require("ethers");
const env = require("./_env");
const { runAction } = require("./lit");

const ERC20_ABI = [
  "function mint(address to, uint256 amount)",
  "function approve(address spender, uint256 amount) returns (bool)",
  "function allowance(address owner, address spender) view returns (uint256)",
];
const SETTLEMENT_ABI = [
  "function depositBase(uint256 epoch, uint256 amount)",
  "function depositQuote(uint256 epoch, uint256 amount)",
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
  const settlement = process.env.SETTLEMENT_ADDRESS;
  const chainId = Number(process.env.CHAIN_ID || "84532");
  for (const [k, v] of [
    ["LIT_USAGE_API_KEY", usageKey],
    ["VAULT_PKP_ADDRESS", pkpId],
    ["ENCRYPTED_DATABASE_URL", encryptedDbUrl],
    ["SETTLEMENT_ADDRESS", settlement],
  ]) {
    if (!v) throw new Error(`${k} missing — run \`npm run setup\` (with DEPLOYER_PRIVATE_KEY set) first`);
  }

  const side = arg("side");
  if (side !== "buy" && side !== "sell") throw new Error("--side must be buy or sell");
  const price = arg("price");
  const qty = arg("qty");
  if (!price || !qty) throw new Error("--price and --qty are required");
  const epoch = Number(arg("epoch", "1"));

  const key = arg("key", process.env.DEPLOYER_PRIVATE_KEY);
  if (!key) throw new Error("a trader key is required to sign the order (--key or DEPLOYER_PRIVATE_KEY)");
  const signer = new ethers.Wallet(key);
  const trader = signer.address;

  const limitPrice = ethers.utils.parseUnits(price, 18).toString();
  const quantity = ethers.utils.parseUnits(qty, 18).toString();
  const nonce = ethers.BigNumber.from(crypto.randomBytes(32)).toString();

  // Trader signs the order (must match matchEpoch's orderDigest exactly).
  const pairHash = ethers.utils.keccak256(ethers.utils.toUtf8Bytes(pair));
  const orderDigest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["uint256", "address", "uint256", "bytes32", "bool", "uint256", "uint256", "uint256"],
      [chainId, settlement, epoch, pairHash, side === "buy", limitPrice, quantity, nonce]
    )
  );
  const sig = await signer.signMessage(ethers.utils.arrayify(orderDigest));

  // 1. Seal + store the signed order (encrypt + INSERT happen inside the action).
  const code = fs.readFileSync(path.join(__dirname, "..", "action", "encryptOrder.js"), "utf8");
  const res = await runAction(base, usageKey, code, {
    pkpId,
    encryptedDbUrl,
    epoch,
    pair,
    order: { side, limitPrice, quantity, trader, nonce, sig },
  });
  if (!res || !res.ok) throw new Error(`encryptOrder rejected: ${JSON.stringify(res)}`);
  console.log(`sealed ${side} ${qty} @ ${price} for ${trader} -> order #${res.id} (epoch ${epoch})`);

  // 2. Escrow the backing tokens for this epoch (locked until the epoch settles).
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
  // Public RPCs are load-balanced and can lag: a just-mined approval may not be
  // visible on the node that answers the deposit's gas estimate yet. Wait until
  // the allowance reads back before depositing.
  for (let i = 0; i < 20; i++) {
    if ((await token.allowance(trader, settlement)).gte(amount)) break;
    await new Promise((r) => setTimeout(r, 1500));
  }
  const sc = new ethers.Contract(settlement, SETTLEMENT_ABI, w);
  await (await (isSell ? sc.depositBase(epoch, amount) : sc.depositQuote(epoch, amount))).wait();
  console.log(`escrowed ${ethers.utils.formatUnits(amount, 18)} ${isSell ? "base" : "quote"} to epoch ${epoch}`);
}

main().catch((err) => {
  console.error("submit failed:", err.message);
  process.exit(1);
});
