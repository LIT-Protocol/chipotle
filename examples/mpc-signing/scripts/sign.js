// Sign + execute a vault call with the threshold key. Two signing modes, two
// targets:
//
//   Modes:
//     (default)    hot share + Lit Action  — the normal path (4 MPC rounds).
//     --recovery   hot share + cold share  — NO Lit Action; entirely local.
//                  The 2-of-3 self-custody escape hatch (run `keygen
//                  --with-recovery` first, and restore the cold share).
//   Targets:
//     (default)    submit vault.exec(...) on-chain.
//     --dry        produce + verify the signature locally; no chain, no funds.
//
// Either way the result is a standard secp256k1 ECDSA signature the vault
// verifies with plain ecrecover.
//
// Usage:
//   node scripts/sign.js --to 0xRecipient --value 0.001 [--data 0x]
//   node scripts/sign.js --to 0xRecipient --value 0.001 --recovery
//   node scripts/sign.js --dry            (normal-path Lit signing, no chain)
//   node scripts/sign.js --dry --recovery (hot+cold signing, no chain)

const { ethers } = require("ethers");
const env = require("./_env");
env.load();

const { MpcClient } = require("../client/mpcClient");
const store = require("../client/store");

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_USAGE_API_KEY,
  MPC_PKP_ADDRESS,
  VAULT_ADDRESS,
  VAULT_SIGNER_ADDRESS,
  RPC_URL = "https://sepolia.base.org",
  EXECUTOR_PRIVATE_KEY,
} = process.env;

// secp256k1 group order; OpenZeppelin's ECDSA rejects s > N/2 (malleability).
const SECP256K1_N = ethers.BigNumber.from(
  "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141"
);

function parseArgs() {
  const out = {};
  const a = process.argv.slice(2);
  for (let i = 0; i < a.length; i++) {
    const key = a[i].replace(/^--/, "");
    const next = a[i + 1];
    if (next === undefined || next.startsWith("--")) {
      out[key] = true; // valueless flag (--dry, --recovery)
    } else {
      out[key] = next; // key + value (--to, --value, --data)
      i++;
    }
  }
  return out;
}

// Normalize to low-s and find the recovery id that yields `expected`.
function assembleSignature(r, s, ethSigned, expected) {
  let sBn = ethers.BigNumber.from(s);
  if (sBn.gt(SECP256K1_N.div(2))) sBn = SECP256K1_N.sub(sBn);
  const sHex = ethers.utils.hexZeroPad(sBn.toHexString(), 32);
  for (const v of [27, 28]) {
    if (ethers.utils.recoverAddress(ethSigned, { r, s: sHex, v }).toLowerCase() === expected.toLowerCase()) {
      return ethers.utils.joinSignature({ r, s: sHex, v });
    }
  }
  throw new Error("could not recover the expected signer from the MPC signature");
}

// Produce { r, s } either via hot+Lit (normal) or hot+cold (recovery).
async function produceSig(recovery, messageHash) {
  const hot = store.load();
  if (recovery) {
    const cold = store.loadCold();
    console.log("Recovery signing: hot + cold shares, NO Lit Action involved...");
    return MpcClient.signLocal({
      shares: [
        { bytes: hot.hotShare, party: hot.hotParty ?? 0 },
        { bytes: cold.coldShare, party: cold.coldParty ?? 2 },
      ],
      messageHash,
      chainPath: hot.chainPath || "m",
    });
  }
  console.log("Signing: hot share + Lit Action (4 rounds)...");
  const mpc = new MpcClient({ apiBase: LIT_API_BASE, usageApiKey: LIT_USAGE_API_KEY, pkpId: MPC_PKP_ADDRESS });
  const sig = await mpc.sign({
    hotShare: hot.hotShare,
    encActionKeyshare: hot.encActionKeyshare,
    chainPath: hot.chainPath || "m",
    messageHash,
    onRound: (rnd) => process.stdout.write(`  round ${rnd}/4\r`),
  });
  process.stdout.write("\n");
  return sig;
}

async function main() {
  const args = parseArgs();
  const recovery = "recovery" in args;
  const dry = "dry" in args;

  if (!recovery) {
    for (const k of ["LIT_USAGE_API_KEY", "MPC_PKP_ADDRESS"]) {
      if (!process.env[k]) throw new Error(`${k} is required for hot+Lit signing`);
    }
  }

  let ethSigned, messageHash, expected, submit = null;

  if (dry) {
    if (!VAULT_SIGNER_ADDRESS) throw new Error("VAULT_SIGNER_ADDRESS is required (run `npm run keygen` first)");
    expected = VAULT_SIGNER_ADDRESS;
    const inner = ethers.utils.keccak256(ethers.utils.toUtf8Bytes(`mpc-signing ${recovery ? "recovery " : ""}dry run`));
    ethSigned = ethers.utils.hashMessage(ethers.utils.arrayify(inner));
    messageHash = ethers.utils.arrayify(ethSigned);
    console.log(`Dry run (${recovery ? "recovery: hot+cold" : "normal: hot+Lit"}) — no chain.\n`);
  } else {
    if (!args.to) throw new Error("Usage: node scripts/sign.js --to 0x.. --value 0.001 [--data 0x] [--recovery] [--dry]");
    for (const k of ["VAULT_ADDRESS", "EXECUTOR_PRIVATE_KEY"]) {
      if (!process.env[k]) throw new Error(`${k} is required`);
    }
    const to = ethers.utils.getAddress(args.to);
    const value = ethers.utils.parseEther(args.value || "0");
    const data = args.data || "0x";

    const provider = new ethers.providers.JsonRpcProvider(RPC_URL);
    const executor = new ethers.Wallet(EXECUTOR_PRIVATE_KEY, provider);
    const vault = new ethers.Contract(
      VAULT_ADDRESS,
      [
        "function signer() view returns (address)",
        "function nonce() view returns (uint256)",
        "function digest(address to, uint256 value, bytes data) view returns (bytes32)",
        "function exec(address to, uint256 value, bytes data, bytes signature)",
      ],
      executor
    );

    const nonce = await vault.nonce();
    const inner = await vault.digest(to, value, data);
    ethSigned = ethers.utils.hashMessage(ethers.utils.arrayify(inner)); // EIP-191
    messageHash = ethers.utils.arrayify(ethSigned);
    expected = VAULT_SIGNER_ADDRESS || (await vault.signer());

    const bal = await provider.getBalance(VAULT_ADDRESS);
    console.log(`Vault:    ${VAULT_ADDRESS}  (balance ${ethers.utils.formatEther(bal)} ETH)`);
    console.log(`Signer:   ${expected}`);
    console.log(`Exec:     to=${to} value=${ethers.utils.formatEther(value)} nonce=${nonce}`);
    if (bal.lt(value)) console.log("\n⚠️  Vault balance is below the requested value — fund it first.");
    console.log();

    submit = async (signature) => {
      const tx = await vault.exec(to, value, data, signature);
      console.log("\ntx:", tx.hash);
      const receipt = await tx.wait();
      console.log("mined in block", receipt.blockNumber);
    };
  }

  const { r, s } = await produceSig(recovery, messageHash);
  const signature = assembleSignature(r, s, ethSigned, expected);
  console.log(`✓ signature recovers to ${expected} — ecrecover-valid.`);
  console.log(`  signature: ${signature}`);

  if (submit) await submit(signature);
}

main().catch((err) => {
  console.error("\nSign failed:", err.message);
  process.exit(1);
});
