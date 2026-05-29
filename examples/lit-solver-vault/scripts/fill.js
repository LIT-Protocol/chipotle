// Happy path. A legit order is on the books; the solver bot asks the policy
// action to authorize a fill, then submits executeFill with the returned
// signature. Funds move from the vault to the order's recipient.
//
//   1. policy action reads the order on-chain, binds the fill to it, signs
//   2. SolverVault.executeFill verifies the signature, releases inventory
//
// Usage: node scripts/fill.js   (or: npm run fill)

const { ethers } = require("ethers");
const env = require("./_env");
env.load();
const { requestFillAuthorization, fillParams } = require("./_lit");

async function main() {
  for (const k of [
    "SOLVER_VAULT_ADDRESS",
    "MOCK_USDC_ADDRESS",
    "MOCK_SETTLEMENT_ADDRESS",
    "SAMPLE_DEPOSIT_ID",
    "SOLVER_PRIVATE_KEY",
    "ALCHEMY_BASE_SEPOLIA_URL",
  ]) {
    if (!process.env[k]) throw new Error(`${k} is required (run \`npm run setup\`)`);
  }

  const params = fillParams();
  console.log(`Requesting fill authorization: 100 mUSDC -> ${params.recipient}`);

  const t0 = Date.now();
  const auth = await requestFillAuthorization(params);
  const ms = Date.now() - t0;

  if (!auth || !auth.authorized) {
    console.error("Policy DENIED the fill:", auth && auth.reason);
    process.exit(2);
  }
  console.log(`Policy authorized in ${ms}ms. Signer: ${auth.signer}`);

  const provider = new ethers.providers.JsonRpcProvider(process.env.ALCHEMY_BASE_SEPOLIA_URL);
  const solver = new ethers.Wallet(process.env.SOLVER_PRIVATE_KEY, provider);
  const vault = new ethers.Contract(
    process.env.SOLVER_VAULT_ADDRESS,
    [
      "function executeFill(address token, address recipient, uint256 amount, bytes32 nonce, uint256 deadline, bytes signature)",
    ],
    solver
  );

  console.log("Submitting executeFill...");
  const tx = await vault.executeFill(
    params.token,
    params.recipient,
    params.amount,
    params.nonce,
    params.deadline,
    auth.signature
  );
  console.log("tx:", tx.hash);
  const receipt = await tx.wait();
  console.log("Fill landed in block", receipt.blockNumber);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
