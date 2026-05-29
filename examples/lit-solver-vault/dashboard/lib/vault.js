// Server-side helpers for the dashboard. Everything here runs in Next.js API
// routes (Node), never the browser — the RPC URL and owner key stay server-side.
//
// We talk to the RPC with plain `fetch` JSON-RPC and use ethers only for ABI
// encode/decode. ethers v5's own provider transport ("could not detect
// network" / "missing response") doesn't survive Next's bundling + fetch
// patching, and this is the same lightweight pattern the Lit Actions use.

const { ethers } = require("ethers");

const VAULT_IFACE = new ethers.utils.Interface([
  "function killSwitch() view returns (bool)",
  "function maxFillAmount() view returns (uint256)",
  "function owner() view returns (address)",
  "function coldWallet() view returns (address)",
  "function policySigner() view returns (address)",
  "function spokePool() view returns (address)",
  "function setKillSwitch(bool on)",
  "event AcrossFillExecuted(uint32 indexed depositId, uint256 indexed originChainId, address indexed recipient, address outputToken, uint256 outputAmount)",
]);

const ERC20_IFACE = new ethers.utils.Interface([
  "function balanceOf(address) view returns (uint256)",
  "function symbol() view returns (string)",
  "function decimals() view returns (uint8)",
]);

const CHAIN_ID = 84532;

function cfg() {
  const rpc = process.env.ALCHEMY_BASE_SEPOLIA_URL;
  const vault = process.env.VAULT_ADDRESS;
  const token = process.env.TOKEN_ADDRESS || "0x4200000000000000000000000000000000000006";
  if (!rpc) throw new Error("ALCHEMY_BASE_SEPOLIA_URL is not set (copy .env.local.example)");
  if (!vault) throw new Error("VAULT_ADDRESS is not set (copy ACROSS_VAULT_ADDRESS from ../.env)");
  return { rpc, vault, token };
}

async function rpc(method, params) {
  const res = await fetch(cfg().rpc, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    cache: "no-store",
  });
  const body = await res.json();
  if (body.error) throw new Error(`${method}: ${body.error.message}`);
  return body.result;
}

async function callView(to, iface, fn, args = []) {
  const data = iface.encodeFunctionData(fn, args);
  const result = await rpc("eth_call", [{ to, data }, "latest"]);
  const decoded = iface.decodeFunctionResult(fn, result);
  return decoded.length === 1 ? decoded[0] : decoded;
}

const hexBlock = (n) => "0x" + n.toString(16);

// Read the full ops snapshot the dashboard renders.
async function getState() {
  const { vault, token } = cfg();

  const head = parseInt(await rpc("eth_blockNumber", []), 16);
  const lookback = Number(process.env.FILL_LOOKBACK_BLOCKS || 50000);
  const fromBlock = Math.max(0, head - lookback);
  const topic0 = VAULT_IFACE.getEventTopic("AcrossFillExecuted");

  const [
    killSwitch,
    maxFillAmount,
    owner,
    coldWallet,
    policySigner,
    spokePool,
    balance,
    symbol,
    decimals,
    logs,
  ] = await Promise.all([
    callView(vault, VAULT_IFACE, "killSwitch"),
    callView(vault, VAULT_IFACE, "maxFillAmount"),
    callView(vault, VAULT_IFACE, "owner"),
    callView(vault, VAULT_IFACE, "coldWallet"),
    callView(vault, VAULT_IFACE, "policySigner"),
    callView(vault, VAULT_IFACE, "spokePool"),
    callView(token, ERC20_IFACE, "balanceOf", [vault]),
    callView(token, ERC20_IFACE, "symbol").catch(() => "TOKEN"),
    callView(token, ERC20_IFACE, "decimals").catch(() => 18),
    rpc("eth_getLogs", [
      { address: vault, topics: [topic0], fromBlock: hexBlock(fromBlock), toBlock: "latest" },
    ]),
  ]);

  const recent = logs.slice(-25).reverse();

  // Block timestamps for the rows we show.
  const tsMap = {};
  await Promise.all(
    [...new Set(recent.map((l) => l.blockNumber))].map(async (bn) => {
      const b = await rpc("eth_getBlockByNumber", [bn, false]);
      tsMap[bn] = parseInt(b.timestamp, 16);
    })
  );

  const fills = recent.map((l) => {
    const p = VAULT_IFACE.parseLog({ topics: l.topics, data: l.data });
    return {
      depositId: p.args.depositId.toString(),
      originChainId: p.args.originChainId.toString(),
      recipient: p.args.recipient,
      outputToken: p.args.outputToken,
      amount: ethers.utils.formatUnits(p.args.outputAmount, decimals),
      txHash: l.transactionHash,
      block: parseInt(l.blockNumber, 16),
      timestamp: tsMap[l.blockNumber] || null,
    };
  });

  return {
    vault,
    token,
    symbol,
    chainId: CHAIN_ID,
    killSwitch,
    maxFillAmount: ethers.utils.formatUnits(maxFillAmount, decimals),
    inventory: ethers.utils.formatUnits(balance, decimals),
    owner,
    coldWallet,
    policySigner,
    spokePool,
    fills,
    asOfBlock: head,
  };
}

// Flip the kill switch. Requires OWNER_PRIVATE_KEY server-side. Builds, signs,
// and broadcasts the tx over plain JSON-RPC (no ethers provider transport).
async function setKillSwitch(on) {
  const key = process.env.OWNER_PRIVATE_KEY;
  if (!key) throw new Error("OWNER_PRIVATE_KEY not set — dashboard is read-only");
  const { vault } = cfg();
  const wallet = new ethers.Wallet(key);
  const data = VAULT_IFACE.encodeFunctionData("setKillSwitch", [Boolean(on)]);

  const [nonceHex, gasHex, latest, tipHex] = await Promise.all([
    rpc("eth_getTransactionCount", [wallet.address, "pending"]),
    rpc("eth_estimateGas", [{ from: wallet.address, to: vault, data }]),
    rpc("eth_getBlockByNumber", ["latest", false]),
    rpc("eth_maxPriorityFeePerGas", []).catch(() => null),
  ]);

  const baseFee = ethers.BigNumber.from(latest.baseFeePerGas || "0x0");
  const tip = tipHex ? ethers.BigNumber.from(tipHex) : ethers.utils.parseUnits("1", "gwei");
  const tx = {
    to: vault,
    data,
    chainId: CHAIN_ID,
    type: 2,
    nonce: parseInt(nonceHex, 16),
    gasLimit: ethers.BigNumber.from(gasHex).mul(12).div(10),
    maxPriorityFeePerGas: tip,
    maxFeePerGas: baseFee.mul(2).add(tip),
  };
  const signed = await wallet.signTransaction(tx);
  const txHash = await rpc("eth_sendRawTransaction", [signed]);

  // Poll briefly for the receipt so the UI reflects the new state on refresh.
  for (let i = 0; i < 20; i++) {
    const r = await rpc("eth_getTransactionReceipt", [txHash]).catch(() => null);
    if (r) return { txHash, block: parseInt(r.blockNumber, 16), status: parseInt(r.status, 16) };
    await new Promise((res) => setTimeout(res, 1500));
  }
  return { txHash, pending: true };
}

module.exports = { getState, setKillSwitch };
