// Deploys the self-contained CoW stack on Base Sepolia:
//   1. GPv2AllowListAuthentication (our own instance) + initializeManager(deployer)
//   2. GPv2Settlement(authenticator, balancerVault)  (creates its VaultRelayer)
//   3. two MockERC20s: a sell token (the trader sells) + a buy token (vault inventory)
//   4. CowSolverVault (pins settlement, policy signer, owner, cold wallet, cap)
//   5. allowlist the vault as a solver on our authenticator
//   6. fund the vault with buy-token inventory
//
// Steps 1-2 deploy the *real* GPv2 contracts from the @cowprotocol/contracts
// published artifacts (solc 0.7.6 bytecode), via an ethers ContractFactory.
// CowSolverVault / MockERC20 are our 0.8.24 contracts.
//
// Invoked by setup-cow.js via execSync once COW_POLICY_SIGNER_ADDRESS exists.

const hre = require("hardhat");
const env = require("./_env");
const { BALANCER_VAULT, cowArtifact } = require("./_cow");

// Alchemy is load-balanced and lags read-after-write: a contract just mined may
// not be visible to the node that estimates the next tx's gas, so ethers can
// estimate a call to it as a no-op (codeless) and the real tx then runs out of
// gas. Poll until the code is visible before calling into a freshly-deployed
// contract, and still pass explicit gas limits on mutating calls below.
async function waitForCode(addr, tries = 20) {
  for (let i = 0; i < tries; i++) {
    if ((await hre.ethers.provider.getCode(addr)) !== "0x") return;
    await new Promise((r) => setTimeout(r, 1500));
  }
  throw new Error(`timed out waiting for code at ${addr}`);
}

async function main() {
  env.load();

  const policySigner = process.env.COW_POLICY_SIGNER_ADDRESS;
  if (!policySigner) {
    throw new Error("COW_POLICY_SIGNER_ADDRESS is required (run `npm run setup`)");
  }

  const [deployer] = await hre.ethers.getSigners();
  const owner = deployer.address;
  const coldWallet = process.env.COLD_WALLET || owner;

  console.log("Deployer / owner:", owner);
  console.log("Cold wallet:     ", coldWallet);
  console.log("Policy signer:   ", policySigner);
  console.log("Balancer vault:  ", BALANCER_VAULT, "(never called for erc20 orders)");

  // 1. Our own allowlist authenticator.
  const authArt = cowArtifact("GPv2AllowListAuthentication");
  const Auth = new hre.ethers.ContractFactory(authArt.abi, authArt.bytecode, deployer);
  const auth = await Auth.deploy();
  await auth.deployed();
  await waitForCode(auth.address);
  await (await auth.initializeManager(owner, { gasLimit: 120000 })).wait();
  console.log("GPv2AllowListAuthentication:", auth.address, "(manager =", owner + ")");

  // 2. Our own settlement. It deploys its VaultRelayer in-constructor.
  const settArt = cowArtifact("GPv2Settlement");
  const Settlement = new hre.ethers.ContractFactory(settArt.abi, settArt.bytecode, deployer);
  const settlement = await Settlement.deploy(auth.address, BALANCER_VAULT);
  await settlement.deployed();
  await waitForCode(settlement.address);
  const vaultRelayer = await settlement.vaultRelayer();
  console.log("GPv2Settlement:             ", settlement.address);
  console.log("  VaultRelayer:             ", vaultRelayer);

  // 3. Test tokens: 6-decimal "USDC" the trader sells, 18-decimal "WETH"
  //    inventory the vault pays out — shows the math is decimal-agnostic.
  const Mock = await hre.ethers.getContractFactory("MockERC20");
  const sellToken = await Mock.deploy("Mock USD Coin", "mUSDC", 6);
  await sellToken.deployed();
  const buyToken = await Mock.deploy("Mock Wrapped Ether", "mWETH", 18);
  await buyToken.deployed();
  console.log("Sell token (mUSDC, 6dp):    ", sellToken.address);
  console.log("Buy token  (mWETH, 18dp):   ", buyToken.address);

  // 4. The vault — the allowlisted solver. maxFillAmount is in buy-token units.
  const maxFill = hre.ethers.utils.parseEther(process.env.COW_MAX_FILL_WETH || "0.05");
  const Vault = await hre.ethers.getContractFactory("CowSolverVault");
  const vault = await Vault.deploy(
    settlement.address,
    policySigner,
    owner,
    coldWallet,
    maxFill,
    sellToken.address,
    buyToken.address
  );
  await vault.deployed();
  console.log("CowSolverVault:             ", vault.address);

  // 5. Allowlist the vault as a solver on our authenticator.
  await (await auth.addSolver(vault.address, { gasLimit: 120000 })).wait();
  // isSolver right after the tx can read stale on a lagging node; poll briefly.
  let isSolver = false;
  for (let i = 0; i < 10 && !isSolver; i++) {
    isSolver = await auth.isSolver(vault.address);
    if (!isSolver) await new Promise((r) => setTimeout(r, 1500));
  }
  console.log("Allowlisted vault as solver:", isSolver);

  // 6. Fund the vault with buy-token inventory.
  const inventory = hre.ethers.utils.parseEther(process.env.COW_INVENTORY_WETH || "0.1");
  await waitForCode(buyToken.address);
  await (await buyToken.mint(vault.address, inventory, { gasLimit: 120000 })).wait();
  console.log(`Funded vault with ${hre.ethers.utils.formatEther(inventory)} mWETH inventory`);

  env.upsert("COW_AUTH_ADDRESS", auth.address);
  env.upsert("COW_SETTLEMENT_ADDRESS", settlement.address);
  env.upsert("COW_VAULT_RELAYER", vaultRelayer);
  env.upsert("COW_SELL_TOKEN", sellToken.address);
  env.upsert("COW_BUY_TOKEN", buyToken.address);
  env.upsert("COW_VAULT_ADDRESS", vault.address);
  env.upsert("COLD_WALLET", coldWallet);
  console.log("\nWrote deployed addresses to .env");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
