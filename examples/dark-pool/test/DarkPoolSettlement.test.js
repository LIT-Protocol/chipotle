const { expect } = require("chai");
const { ethers } = require("hardhat");

// Mirrors the digest the matchEpoch action will build and the contract verifies.
// keccak256(abi.encode(epoch, pairHash, clearingPx, keccak256(abi.encode(fills)),
//                      settlement, chainId)) then EIP-191 personal-sign.
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

  let baseToken, quoteToken, settlement, matcher, chainId;
  let buyer, seller;

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
    settlement = await Settlement.deploy(
      baseToken.address,
      quoteToken.address,
      "BASE/QUOTE",
      matcher.address
    );
    await settlement.deployed();

    // Seller escrows base, buyer escrows quote.
    await baseToken.mint(seller.address, qty);
    await quoteToken.mint(buyer.address, cost);
    await baseToken.connect(seller).approve(settlement.address, qty);
    await quoteToken.connect(buyer).approve(settlement.address, cost);
    await settlement.connect(seller).depositBase(qty);
    await settlement.connect(buyer).depositQuote(cost);
  });

  function crossFills() {
    return [
      { trader: buyer.address, isBuy: true, quantity: qty },
      { trader: seller.address, isBuy: false, quantity: qty },
    ];
  }

  it("settles a crossing pair at the clearing price and lets traders withdraw", async function () {
    const fills = crossFills();
    const sig = await signSettlement(matcher, settlement.address, chainId, 1, clearingPx, fills);

    await expect(settlement.settleEpoch(1, clearingPx, fills, sig))
      .to.emit(settlement, "EpochSettled")
      .withArgs(1, clearingPx, 2);

    // Buyer: spent all quote, holds the base. Seller: delivered base, holds quote.
    expect(await settlement.baseBalance(buyer.address)).to.equal(qty);
    expect(await settlement.quoteBalance(buyer.address)).to.equal(0);
    expect(await settlement.baseBalance(seller.address)).to.equal(0);
    expect(await settlement.quoteBalance(seller.address)).to.equal(cost);

    await settlement.connect(buyer).withdrawBase(qty);
    await settlement.connect(seller).withdrawQuote(cost);
    expect(await baseToken.balanceOf(buyer.address)).to.equal(qty);
    expect(await quoteToken.balanceOf(seller.address)).to.equal(cost);
  });

  it("rejects a signature from anyone but the pinned matcher", async function () {
    const imposter = ethers.Wallet.createRandom();
    const fills = crossFills();
    const sig = await signSettlement(imposter, settlement.address, chainId, 1, clearingPx, fills);
    await expect(
      settlement.settleEpoch(1, clearingPx, fills, sig)
    ).to.be.revertedWithCustomError(settlement, "InvalidMatcherSignature");
  });

  it("rejects replaying an already-settled epoch", async function () {
    const fills = crossFills();
    const sig = await signSettlement(matcher, settlement.address, chainId, 1, clearingPx, fills);
    await settlement.settleEpoch(1, clearingPx, fills, sig);
    await expect(
      settlement.settleEpoch(1, clearingPx, fills, sig)
    ).to.be.revertedWithCustomError(settlement, "EpochAlreadySettled");
  });

  it("reverts when a trader is under-collateralised", async function () {
    // Fresh buyer with no quote deposited.
    const [, , , poorBuyer] = await ethers.getSigners();
    const fills = [
      { trader: poorBuyer.address, isBuy: true, quantity: qty },
      { trader: seller.address, isBuy: false, quantity: qty },
    ];
    const sig = await signSettlement(matcher, settlement.address, chainId, 1, clearingPx, fills);
    await expect(settlement.settleEpoch(1, clearingPx, fills, sig)).to.be.reverted; // panic 0x11
  });

  it("rejects fills that don't conserve base (bought != sold)", async function () {
    const fills = [{ trader: buyer.address, isBuy: true, quantity: qty }]; // buy with no matching sell
    const sig = await signSettlement(matcher, settlement.address, chainId, 1, clearingPx, fills);
    await expect(
      settlement.settleEpoch(1, clearingPx, fills, sig)
    ).to.be.revertedWithCustomError(settlement, "ConservationViolated");
  });

  it("binds the signature to this contract address (no cross-pool replay)", async function () {
    const fills = crossFills();
    // Sign for a different settlement address.
    const wrongAddr = ethers.Wallet.createRandom().address;
    const sig = await signSettlement(matcher, wrongAddr, chainId, 1, clearingPx, fills);
    await expect(
      settlement.settleEpoch(1, clearingPx, fills, sig)
    ).to.be.revertedWithCustomError(settlement, "InvalidMatcherSignature");
  });
});
