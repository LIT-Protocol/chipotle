const { expect } = require("chai");
const { ethers } = require("hardhat");

// Mirrors the digest matchEpoch builds and the contract verifies.
const FILLS_TYPE = "tuple(address trader, bool isBuy, uint256 quantity)[]";

async function signSettlement(matcher, settlementAddr, chainId, epoch, clearingPx, fills) {
  const pairHash = ethers.utils.keccak256(ethers.utils.toUtf8Bytes("BASE/QUOTE"));
  const fillsTuples = fills.map((f) => [f.trader, f.isBuy, f.quantity]);
  const fillsHash = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode([FILLS_TYPE], [fillsTuples])
  );
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["uint256", "bytes32", "uint256", "bytes32", "address", "uint256"],
      [epoch, pairHash, clearingPx, fillsHash, settlementAddr, chainId]
    )
  );
  return matcher.signMessage(ethers.utils.arrayify(digest));
}

describe("DarkPoolSettlement", function () {
  const PRICE_SCALE = ethers.utils.parseUnits("1", 18);
  const clearingPx = ethers.utils.parseUnits("100", 18); // 100 quote per base
  const qty = ethers.utils.parseUnits("5", 18); // 5 base
  const cost = qty.mul(clearingPx).div(PRICE_SCALE); // 500 quote
  const EPOCH = 1;

  let baseToken, quoteToken, settlement, matcher, chainId;
  let buyer, seller;

  async function deposit(signer, isSell, amount) {
    const token = isSell ? baseToken : quoteToken;
    await token.mint(signer.address, amount);
    await token.connect(signer).approve(settlement.address, amount);
    await (isSell
      ? settlement.connect(signer).depositBase(EPOCH, amount)
      : settlement.connect(signer).depositQuote(EPOCH, amount));
  }

  beforeEach(async function () {
    [, buyer, seller] = await ethers.getSigners();
    matcher = ethers.Wallet.createRandom();
    chainId = (await ethers.provider.getNetwork()).chainId;

    const Token = await ethers.getContractFactory("TestToken");
    baseToken = await Token.deploy("Base", "BASE");
    quoteToken = await Token.deploy("Quote", "QUOTE");
    await baseToken.deployed();
    await quoteToken.deployed();

    const Settlement = await ethers.getContractFactory("DarkPoolSettlement");
    settlement = await Settlement.deploy(baseToken.address, quoteToken.address, "BASE/QUOTE", matcher.address);
    await settlement.deployed();

    await deposit(seller, true, qty); // seller escrows base
    await deposit(buyer, false, cost); // buyer escrows quote
  });

  function crossFills() {
    return [
      { trader: buyer.address, isBuy: true, quantity: qty },
      { trader: seller.address, isBuy: false, quantity: qty },
    ];
  }

  it("settles a crossing pair and lets traders withdraw proceeds", async function () {
    const fills = crossFills();
    const sig = await signSettlement(matcher, settlement.address, chainId, EPOCH, clearingPx, fills);

    await expect(settlement.settleEpoch(EPOCH, clearingPx, fills, sig))
      .to.emit(settlement, "EpochSettled")
      .withArgs(EPOCH, clearingPx, 2);

    expect(await settlement.baseProceeds(buyer.address)).to.equal(qty);
    expect(await settlement.quoteProceeds(seller.address)).to.equal(cost);
    // escrow fully spent
    expect(await settlement.quoteEscrow(EPOCH, buyer.address)).to.equal(0);
    expect(await settlement.baseEscrow(EPOCH, seller.address)).to.equal(0);

    await settlement.connect(buyer).withdrawProceeds();
    await settlement.connect(seller).withdrawProceeds();
    expect(await baseToken.balanceOf(buyer.address)).to.equal(qty);
    expect(await quoteToken.balanceOf(seller.address)).to.equal(cost);
  });

  it("rejects a signature from anyone but the pinned matcher", async function () {
    const imposter = ethers.Wallet.createRandom();
    const fills = crossFills();
    const sig = await signSettlement(imposter, settlement.address, chainId, EPOCH, clearingPx, fills);
    await expect(settlement.settleEpoch(EPOCH, clearingPx, fills, sig)).to.be.revertedWithCustomError(
      settlement,
      "InvalidMatcherSignature"
    );
  });

  it("rejects replaying an already-settled epoch", async function () {
    const fills = crossFills();
    const sig = await signSettlement(matcher, settlement.address, chainId, EPOCH, clearingPx, fills);
    await settlement.settleEpoch(EPOCH, clearingPx, fills, sig);
    await expect(settlement.settleEpoch(EPOCH, clearingPx, fills, sig)).to.be.revertedWithCustomError(
      settlement,
      "EpochAlreadySettled"
    );
  });

  it("reverts when a trader is under-collateralised for the epoch", async function () {
    const [, , , poorBuyer] = await ethers.getSigners();
    const fills = [
      { trader: poorBuyer.address, isBuy: true, quantity: qty }, // no escrow in EPOCH
      { trader: seller.address, isBuy: false, quantity: qty },
    ];
    const sig = await signSettlement(matcher, settlement.address, chainId, EPOCH, clearingPx, fills);
    await expect(settlement.settleEpoch(EPOCH, clearingPx, fills, sig)).to.be.reverted; // panic 0x11
  });

  it("rejects fills that don't conserve base (bought != sold)", async function () {
    const fills = [{ trader: buyer.address, isBuy: true, quantity: qty }];
    const sig = await signSettlement(matcher, settlement.address, chainId, EPOCH, clearingPx, fills);
    await expect(settlement.settleEpoch(EPOCH, clearingPx, fills, sig)).to.be.revertedWithCustomError(
      settlement,
      "ConservationViolated"
    );
  });

  it("binds the signature to this contract address (no cross-pool replay)", async function () {
    const fills = crossFills();
    const wrongAddr = ethers.Wallet.createRandom().address;
    const sig = await signSettlement(matcher, wrongAddr, chainId, EPOCH, clearingPx, fills);
    await expect(settlement.settleEpoch(EPOCH, clearingPx, fills, sig)).to.be.revertedWithCustomError(
      settlement,
      "InvalidMatcherSignature"
    );
  });

  it("locks escrow until the epoch settles (no withdraw-before-settle griefing)", async function () {
    // The buyer is matched in EPOCH; trying to pull escrow before settlement must fail.
    await expect(settlement.connect(buyer).withdrawEscrow(EPOCH)).to.be.revertedWithCustomError(
      settlement,
      "EpochNotSettled"
    );
    const fills = crossFills();
    const sig = await signSettlement(matcher, settlement.address, chainId, EPOCH, clearingPx, fills);
    await settlement.settleEpoch(EPOCH, clearingPx, fills, sig); // still settles fine
    expect(await settlement.baseProceeds(buyer.address)).to.equal(qty);
  });

  it("refuses deposits into an already-settled epoch", async function () {
    const fills = crossFills();
    const sig = await signSettlement(matcher, settlement.address, chainId, EPOCH, clearingPx, fills);
    await settlement.settleEpoch(EPOCH, clearingPx, fills, sig);
    await quoteToken.mint(buyer.address, cost);
    await quoteToken.connect(buyer).approve(settlement.address, cost);
    await expect(settlement.connect(buyer).depositQuote(EPOCH, cost)).to.be.revertedWithCustomError(
      settlement,
      "EpochAlreadySettled"
    );
  });

  it("refunds over-collateralised escrow after settlement", async function () {
    // Buyer adds an extra 100 quote beyond the 500 needed.
    const extra = ethers.utils.parseUnits("100", 18);
    await deposit(buyer, false, extra);
    const fills = crossFills();
    const sig = await signSettlement(matcher, settlement.address, chainId, EPOCH, clearingPx, fills);
    await settlement.settleEpoch(EPOCH, clearingPx, fills, sig);
    expect(await settlement.quoteEscrow(EPOCH, buyer.address)).to.equal(extra);
    await settlement.connect(buyer).withdrawEscrow(EPOCH);
    expect(await quoteToken.balanceOf(buyer.address)).to.equal(extra);
  });
});
