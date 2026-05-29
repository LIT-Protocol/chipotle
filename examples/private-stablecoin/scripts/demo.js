// The 2-minute privUSD demo. Run after `npm run setup`.
//
// Story:
//   1. KYC + mint:  Alice verifies once, deposits 1,000 USDC -> a private
//                   1,000 privUSD note. (KYC at the dollar edge.)
//   2. Reserve proof: show totalSupply and that the reserve fully backs it.
//   3. Shielded transfer: Alice privately pays Bob 250. The chain shows new
//      commitments + a nullifier — no amount, no parties.
//   4. Disclosure: a regulator with a 3-of-5 warrant decrypts ONLY Bob's
//      note ($250) — every other balance stays dark. This is the money shot.
//
// Notes the wallets hold are tracked in-memory here; a real client would
// reconstruct them by scanning the NoteCreated events it can decrypt.

const fs = require("fs");
const path = require("path");
const { ethers } = require("ethers");
const env = require("./_env");
env.load();

const notes = require("./lib/notes");
const { buildActions } = require("./lib/buildAction");
const { PRIVUSD_ABI, MOCK_USDC_ABI, callAction, rand32, deadlineIn } = require("./lib/litClient");

const {
  LIT_API_BASE = "https://api.chipotle.litprotocol.com",
  LIT_USAGE_API_KEY,
  LEDGER_PKP_ADDRESS,
  PRIVUSD_ADDRESS,
  MOCK_USDC_ADDRESS,
  KYC_SIGNER_PRIVATE_KEY,
  KYC_SIGNER_ADDRESS,
  CHAIN_ID = "84532",
  RPC_URL = "https://sepolia.base.org",
  SCREENING_RPC_URL,
  DEPLOYER_PRIVATE_KEY,
} = process.env;

// Build the actions with the ledger PKP baked in — must match what setup.js
// registered (same source → same CID).
const { ledgerCode, discloseCode } = buildActions(LEDGER_PKP_ADDRESS || "__LEDGER_PKP_ID__");

const USDC = (n) => ethers.utils.parseUnits(String(n), 6);
const fmt = (bn) => ethers.utils.formatUnits(bn, 6);
const explorer = (h) => `https://sepolia.basescan.org/tx/${h}`;

// Explicit gas limits so ethers skips eth_estimateGas. Estimation runs against
// `latest` state, which on a freshly-mined approve/mint can lag a block and
// revert the pre-flight even though the real tx (mined a block or two later)
// succeeds. Fixed limits sidestep the race; values are comfortably above
// measured usage (mint ~145k, transfer ~190k, approve ~46k).
const GAS = { approve: 80000, mint: 280000, transfer: 340000 };

function banner(title) {
  console.log(`\n${"─".repeat(64)}\n${title}\n${"─".repeat(64)}`);
}

// Wait until the contract reports a commitment as live, so the next action's
// eth_call (which validates against `latest`) doesn't race a just-mined block.
async function waitForCommitment(priv, commitment, tries = 20) {
  for (let i = 0; i < tries; i++) {
    if (await priv.commitments(commitment)) return;
    await new Promise((r) => setTimeout(r, 1500));
  }
  throw new Error(`commitment never appeared on-chain: ${commitment}`);
}

async function main() {
  for (const k of ["LIT_USAGE_API_KEY", "LEDGER_PKP_ADDRESS", "PRIVUSD_ADDRESS",
    "MOCK_USDC_ADDRESS", "KYC_SIGNER_PRIVATE_KEY", "SCREENING_RPC_URL", "DEPLOYER_PRIVATE_KEY"]) {
    if (!process.env[k]) throw new Error(`${k} missing in .env — run \`npm run setup\` first`);
  }

  const provider = new ethers.providers.JsonRpcProvider(RPC_URL);
  const operator = new ethers.Wallet(DEPLOYER_PRIVATE_KEY, provider); // issuer operator + Alice's wallet
  const alice = operator.address;
  const bob = ethers.Wallet.createRandom().address; // recipient; only needs an address

  const priv = new ethers.Contract(PRIVUSD_ADDRESS, PRIVUSD_ABI, operator);
  const usdc = new ethers.Contract(MOCK_USDC_ADDRESS, MOCK_USDC_ABI, operator);

  const common = {
    contractAddress: PRIVUSD_ADDRESS,
    chainId: Number(CHAIN_ID),
    contractRpcUrl: RPC_URL,
    screeningRpcUrl: SCREENING_RPC_URL,
  };

  console.log("Alice (issuer-operator wallet):", alice);
  console.log("Bob   (recipient):             ", bob);

  // -------------------------------------------------------------------------
  banner("1. KYC + MINT — Alice deposits 1,000 USDC for 1,000 privUSD");
  // -------------------------------------------------------------------------
  const kycWallet = new ethers.Wallet(KYC_SIGNER_PRIVATE_KEY);
  const kycMessage = JSON.stringify({ subject: alice, status: "passed", exp: deadlineIn(3600) });
  const kycAttestation = { message: kycMessage, signature: await kycWallet.signMessage(kycMessage) };
  console.log(`  KYC attestation signed by ${KYC_SIGNER_ADDRESS} (stands in for Persona/Sumsub)`);

  await (await usdc.approve(PRIVUSD_ADDRESS, USDC(1000), { gasLimit: GAS.approve })).wait();
  console.log("  Alice approved PrivUSD to pull 1,000 USDC");

  const aliceNote = notes.makeNote(alice, USDC(1000));
  const mintNonce = rand32();
  const mintDeadline = deadlineIn(600);
  const mintRes = await callAction({
    base: LIT_API_BASE, usageKey: LIT_USAGE_API_KEY, code: ledgerCode,
    jsParams: {
      ...common, op: "mint", depositor: alice, depositAmount: USDC(1000).toString(),
      outputs: [aliceNote], kycAttestation, kycSigner: KYC_SIGNER_ADDRESS,
      nonce: mintNonce, deadline: mintDeadline,
    },
  });
  console.log("  Lit Action verified KYC + OFAC, signed the mint");
  const mintTx = await priv.mint(alice, USDC(1000), mintRes.commitments, mintRes.encryptedBlobs, mintNonce, mintDeadline, mintRes.signature, { gasLimit: GAS.mint });
  await mintTx.wait();
  await waitForCommitment(priv, notes.commitmentOf(aliceNote));
  console.log("  minted →", explorer(mintTx.hash));

  // -------------------------------------------------------------------------
  banner("2. RESERVE PROOF — public, continuous, no auditor");
  // -------------------------------------------------------------------------
  console.log("  totalSupply:  ", fmt(await priv.totalSupply()), "privUSD");
  console.log("  reserve held: ", fmt(await usdc.balanceOf(PRIVUSD_ADDRESS)), "USDC");
  console.log("  reserveBacked:", await priv.reserveBacked());

  // -------------------------------------------------------------------------
  banner("3. SHIELDED TRANSFER — Alice pays Bob 250, privately");
  // -------------------------------------------------------------------------
  const bobNote = notes.makeNote(bob, USDC(250));
  const aliceChange = notes.makeNote(alice, USDC(750));
  const xferNonce = rand32();
  const xferDeadline = deadlineIn(600);
  const xferRes = await callAction({
    base: LIT_API_BASE, usageKey: LIT_USAGE_API_KEY, code: ledgerCode,
    jsParams: {
      ...common, op: "transfer", caller: alice,
      inputs: [aliceNote], outputs: [bobNote, aliceChange],
      nonce: xferNonce, deadline: xferDeadline,
    },
  });
  console.log("  Lit Action checked the note is live, sum(in)==sum(out), OFAC'd Bob, signed");
  const xferTx = await priv.shieldedTransfer(xferRes.inputNullifiers, xferRes.outputCommitments, xferRes.encryptedBlobs, xferNonce, xferDeadline, xferRes.signature, { gasLimit: GAS.transfer });
  const xferReceipt = await xferTx.wait();
  console.log("  transferred →", explorer(xferTx.hash));
  console.log("\n  What the chain shows for this tx:");
  console.log("    nullifiers spent:   ", xferRes.inputNullifiers);
  console.log("    commitments created:", xferRes.outputCommitments);
  console.log("    → no amount, no sender, no recipient. Just opaque hashes.");

  // Bob's note ciphertext, read from the on-chain NoteCreated event (what a
  // regulator would actually pull). outputs[0] is Bob's note.
  const bobCommitment = xferRes.outputCommitments[0];
  const iface = new ethers.utils.Interface(PRIVUSD_ABI);
  let bobBlob;
  for (const log of xferReceipt.logs) {
    try {
      const parsed = iface.parseLog(log);
      if (parsed.name === "NoteCreated" && parsed.args.commitment === bobCommitment) {
        bobBlob = parsed.args.encryptedBlob;
      }
    } catch {/* not our event */}
  }
  if (!bobBlob) throw new Error("could not find Bob's NoteCreated event");

  // -------------------------------------------------------------------------
  banner("4. DISCLOSURE — a 3-of-5 warrant decrypts ONLY Bob's note");
  // -------------------------------------------------------------------------
  // Five designated disclosure authorities (issuer counsel, outside counsel,
  // a trustee, etc.). A warrant needs 3 of their signatures.
  const authorities = Array.from({ length: 5 }, () => ethers.Wallet.createRandom());
  const authorityAddrs = authorities.map((w) => w.address);
  const warrant = { noteCommitment: bobCommitment, reason: "Court order 2026-CV-1234", expiry: deadlineIn(3600) };
  const warrantMessage = JSON.stringify(warrant);
  const sigs = await Promise.all(authorities.slice(0, 3).map((w) => w.signMessage(warrantMessage)));
  console.log("  3 of 5 authorities co-signed the warrant for Bob's note commitment");

  const discloseRes = await callAction({
    base: LIT_API_BASE, usageKey: LIT_USAGE_API_KEY, code: discloseCode,
    jsParams: { warrantMessage, signatures: sigs, authorities: authorityAddrs, threshold: 3, encryptedBlob: bobBlob },
  });
  console.log("\n  Regulator decrypted exactly one note:");
  console.log("    owner: ", discloseRes.disclosedNote.owner, "(Bob)");
  console.log("    amount:", fmt(discloseRes.disclosedNote.amount), "privUSD");
  console.log("    warrantHash:", discloseRes.warrantHash);
  console.log("\n  Alice's $750 change note and every other balance remain encrypted.");

  // Show that an under-threshold warrant fails closed.
  try {
    await callAction({
      base: LIT_API_BASE, usageKey: LIT_USAGE_API_KEY, code: discloseCode,
      jsParams: { warrantMessage, signatures: sigs.slice(0, 2), authorities: authorityAddrs, threshold: 3, encryptedBlob: bobBlob },
    });
    console.log("  WARNING: 2-of-5 warrant unexpectedly succeeded");
  } catch (e) {
    console.log("  2-of-5 warrant correctly REJECTED:", e.message.split("\n")[0]);
  }

  banner("Demo complete.");
  console.log("Aztec-grade privacy, no ZK circuit. OFAC every hop, KYC at the edges,");
  console.log("provable reserves, and accountable selective disclosure — on a public L2.");
}

main().catch((err) => {
  console.error("\nDemo failed:", err.message);
  process.exit(1);
});
