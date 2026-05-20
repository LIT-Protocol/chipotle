// End-to-end bridge:
//   1. Burn `amount` tokens on the source chain (BridgeToken.burn). The
//      contract emits BurnInitiated(from, recipient, amount, destChainId, nonce).
//   2. Ask the Lit Action to read that receipt over a whitelisted RPC and
//      sign a mint authorization for the destination chain.
//   3. Submit BridgeToken.mint on the destination chain with the signature.
//
// The mint can be submitted by ANYONE — the signature is the authorization,
// not the caller. Here we use the same DEPLOYER_PRIVATE_KEY on both sides
// just because it's already configured; a real deployment might gas-sponsor
// from a relayer wallet or let the recipient submit themselves.
//
// Usage:
//   node scripts/bridge.js --from baseSepolia --to arbitrumSepolia \
//     --amount 25 --recipient 0xRecipient

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");
env.load();

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_USAGE_API_KEY,
  DEPLOYER_PRIVATE_KEY,
  BRIDGE_TOKEN_BASE_SEPOLIA,
  BRIDGE_TOKEN_ARB_SEPOLIA,
  BASE_SEPOLIA_RPC_URL = "https://sepolia.base.org",
  ARBITRUM_SEPOLIA_RPC_URL = "https://sepolia-rollup.arbitrum.io/rpc",
  // RPC URLs the Lit Action is allowed to query — must match the hostname
  // whitelist baked into action/bridgeAction.js (RPC_HOSTS). Defaults to
  // Alchemy endpoints; see the README's "Trust model" section.
  BASE_SEPOLIA_ALCHEMY_URL,
  ARBITRUM_SEPOLIA_ALCHEMY_URL,
} = process.env;

// Hardhat network name -> everything we need to talk to that chain.
// `minConfirmations` must match the per-chain policy baked into
// action/bridgeAction.js (RPC_HOSTS). The action declines to sign until
// the burn block is buried under this many blocks, so bridge.js polls
// up to that depth before calling /lit_action. If you change the action
// table, change this table too.
const NETWORKS = {
  baseSepolia: {
    chainId: 84532,
    address: BRIDGE_TOKEN_BASE_SEPOLIA,
    rpc: BASE_SEPOLIA_RPC_URL, // local provider for the burn / mint tx
    alchemyRpc: BASE_SEPOLIA_ALCHEMY_URL, // the URL the action will use
    minConfirmations: 5,
    label: "Base Sepolia",
  },
  arbitrumSepolia: {
    chainId: 421614,
    address: BRIDGE_TOKEN_ARB_SEPOLIA,
    rpc: ARBITRUM_SEPOLIA_RPC_URL,
    alchemyRpc: ARBITRUM_SEPOLIA_ALCHEMY_URL,
    minConfirmations: 5,
    label: "Arbitrum Sepolia",
  },
};

const BRIDGE_TOKEN_ABI = [
  "function burn(uint256 amount, uint256 destChainId, address recipient) returns (uint256)",
  "function mint(uint256 srcChainId, address srcContract, bytes32 burnTxHash, uint256 logIndex, address recipient, uint256 amount, uint256 srcNonce, uint256 deadline, bytes signature) external",
  "function balanceOf(address) view returns (uint256)",
  "event BurnInitiated(address indexed from, address indexed recipient, uint256 amount, uint256 indexed destChainId, uint256 nonce)",
];

function parseArgs() {
  const out = {};
  for (let i = 2; i < process.argv.length; i += 2) {
    out[process.argv[i].replace(/^--/, "")] = process.argv[i + 1];
  }
  return out;
}

async function main() {
  const args = parseArgs();
  for (const k of ["from", "to", "amount", "recipient"]) {
    if (!args[k]) {
      throw new Error(
        "Usage: node scripts/bridge.js --from baseSepolia --to arbitrumSepolia --amount 25 --recipient 0xRecipient"
      );
    }
  }
  for (const k of ["LIT_USAGE_API_KEY", "DEPLOYER_PRIVATE_KEY"]) {
    if (!process.env[k]) throw new Error(`${k} is required`);
  }
  const src = NETWORKS[args.from];
  const dst = NETWORKS[args.to];
  if (!src) throw new Error(`unknown --from network: ${args.from}`);
  if (!dst) throw new Error(`unknown --to network: ${args.to}`);
  if (!src.address || !dst.address) {
    throw new Error(
      "BRIDGE_TOKEN_* not set in .env — did you run `npm run setup`?"
    );
  }
  if (!src.alchemyRpc) {
    throw new Error(
      `${args.from === "baseSepolia" ? "BASE_SEPOLIA_ALCHEMY_URL" : "ARBITRUM_SEPOLIA_ALCHEMY_URL"} is required ` +
        `— the Lit Action only accepts Alchemy hostnames (see action/bridgeAction.js RPC_HOSTS).`
    );
  }

  const amount = ethers.utils.parseUnits(args.amount, 18);

  // -------------------------------------------------------------------------
  // Step 1: Burn on the source chain.
  // -------------------------------------------------------------------------
  console.log(
    `\nStep 1/3: Burning ${args.amount} tokens on ${src.label} (chainId ${src.chainId})...`
  );
  const srcProvider = new ethers.providers.JsonRpcProvider(src.rpc);
  const srcSigner = new ethers.Wallet(DEPLOYER_PRIVATE_KEY, srcProvider);
  const srcToken = new ethers.Contract(src.address, BRIDGE_TOKEN_ABI, srcSigner);

  const burnTx = await srcToken.burn(amount, dst.chainId, args.recipient);
  console.log(`  burn tx: ${burnTx.hash}`);
  const burnReceipt = await burnTx.wait();
  console.log(`  mined in block ${burnReceipt.blockNumber}`);

  // Find the BurnInitiated log to grab its logIndex (the action needs it
  // to locate the right log on the receipt, since a tx can contain many).
  const iface = new ethers.utils.Interface(BRIDGE_TOKEN_ABI);
  const burnLog = burnReceipt.logs.find((l) => {
    try {
      return (
        l.address.toLowerCase() === src.address.toLowerCase() &&
        iface.parseLog(l).name === "BurnInitiated"
      );
    } catch {
      return false;
    }
  });
  if (!burnLog) throw new Error("burn tx did not emit BurnInitiated");
  const logIndex = burnLog.logIndex;
  const parsed = iface.parseLog(burnLog);
  console.log(
    `  BurnInitiated nonce=${parsed.args.nonce.toString()} logIndex=${logIndex}`
  );

  // -------------------------------------------------------------------------
  // Step 1b: Wait for `minConfirmations` blocks. The action declines to sign
  // until the burn block is buried under N blocks (reorg protection), so
  // calling /lit_action immediately would just bounce. We poll the source
  // provider here instead of inside the action so the failure mode is "the
  // CLI prints a clear progress message" rather than "the action returns
  // an unactionable error."
  const burnBlock = burnReceipt.blockNumber;
  const wantBlock = burnBlock + src.minConfirmations;
  console.log(
    `  waiting for ${src.minConfirmations} confirmations (need head >= block ${wantBlock})...`
  );
  while (true) {
    const head = await srcProvider.getBlockNumber();
    if (head >= wantBlock) {
      console.log(`  head at block ${head} — confirmed`);
      break;
    }
    process.stdout.write(`\r  head at block ${head} (${wantBlock - head} more to go)   `);
    await new Promise((r) => setTimeout(r, 2000));
  }

  // -------------------------------------------------------------------------
  // Step 2: Ask the action to attest the burn and sign the mint.
  // -------------------------------------------------------------------------
  console.log("\nStep 2/3: Asking Lit Action to attest the burn...");
  const deadline = Math.floor(Date.now() / 1000) + 600;
  const actionCode = fs.readFileSync(
    path.join(__dirname, "..", "action", "bridgeAction.js"),
    "utf8"
  );

  const litRes = await fetch(`${LIT_API_BASE}/core/v1/lit_action`, {
    method: "POST",
    headers: {
      "X-Api-Key": LIT_USAGE_API_KEY,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      code: actionCode,
      js_params: {
        burnTxHash: burnTx.hash,
        srcChainId: src.chainId,
        srcRpcUrl: src.alchemyRpc,
        srcContract: src.address,
        destChainId: dst.chainId,
        destContract: dst.address,
        logIndex,
        deadline,
      },
    }),
  });
  const envelope = await litRes.json();
  if (envelope.has_error) {
    console.error("Lit Action errored:", envelope.logs || envelope);
    process.exit(2);
  }
  const body = envelope.response;
  if (!body || !body.authorized) {
    console.error("Action declined to sign:", body || envelope);
    process.exit(2);
  }
  console.log(`  signature: ${body.signature.slice(0, 24)}...`);
  console.log(`  signer:    ${body.signer}`);

  // -------------------------------------------------------------------------
  // Step 3: Submit the mint on the destination chain.
  // -------------------------------------------------------------------------
  console.log(
    `\nStep 3/3: Minting on ${dst.label} (chainId ${dst.chainId})...`
  );
  const dstProvider = new ethers.providers.JsonRpcProvider(dst.rpc);
  const dstSigner = new ethers.Wallet(DEPLOYER_PRIVATE_KEY, dstProvider);
  const dstToken = new ethers.Contract(dst.address, BRIDGE_TOKEN_ABI, dstSigner);

  const mintTx = await dstToken.mint(
    src.chainId,
    src.address,
    burnTx.hash,
    logIndex,
    args.recipient,
    body.amount,
    body.srcNonce,
    deadline,
    body.signature
  );
  console.log(`  mint tx: ${mintTx.hash}`);
  const mintReceipt = await mintTx.wait();
  console.log(`  mined in block ${mintReceipt.blockNumber}`);

  const dstBalance = await dstToken.balanceOf(args.recipient);
  console.log(
    `\n✓ Bridged ${args.amount} tokens from ${src.label} to ${dst.label}.`
  );
  console.log(
    `  Recipient ${args.recipient} balance on ${dst.label}: ${ethers.utils.formatUnits(dstBalance, 18)}`
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
