// Exercises the full on-chain path with a LOCAL signer standing in for the
// Lit Action's CID-derived key. This validates that:
//   - ethers `defaultAbiCoder.encode` matches Solidity `abi.encode` (so the
//     action's digest == the contract's digest),
//   - the EIP-191 sign/recover path lines up,
//   - mint / shieldedTransfer / redeem update state and conserve the reserve,
//   - replay and double-spend are rejected.
// It does NOT touch the Lit network — the action logic is unit-tested by the
// note-crypto equivalence; here we trust a local wallet as the oracle.

const { expect } = require("chai");
const { ethers } = require("hardhat");
const notes = require("../scripts/lib/notes");

const USDC = (n) => ethers.utils.parseUnits(String(n), 6);
const rand32 = () => ethers.utils.hexlify(ethers.utils.randomBytes(32));
const deadline = () => Math.floor(Date.now() / 1000) + 600;

// Mirror the action's signing: keccak256(abi.encode(...)) then EIP-191 sign.
async function sign(oracle, types, values) {
  const digest = ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(types, values));
  return oracle.signMessage(ethers.utils.arrayify(digest));
}

describe("PrivUSD", () => {
  let usdc, priv, deployer, oracle, alice, bob, chainId;

  beforeEach(async () => {
    [deployer] = await ethers.getSigners();
    oracle = ethers.Wallet.createRandom();
    alice = deployer.address;
    bob = ethers.Wallet.createRandom().address;
    chainId = (await ethers.provider.getNetwork()).chainId;

    usdc = await (await ethers.getContractFactory("MockUSDC")).deploy();
    await usdc.deployed();
    await (await usdc.mint(alice, USDC(10000))).wait();

    priv = await (await ethers.getContractFactory("PrivUSD")).deploy(usdc.address, oracle.address);
    await priv.deployed();
    await (await usdc.approve(priv.address, USDC(10000))).wait();
  });

  async function mint(amount, owner = alice) {
    const note = notes.makeNote(owner, USDC(amount));
    const c = [notes.commitmentOf(note)];
    const blobs = ["blob"];
    const nonce = rand32();
    const dl = deadline();
    const sig = await sign(oracle,
      ["string", "address", "uint256", "bytes32[]", "string[]", "bytes32", "uint256", "address", "uint256"],
      ["MINT", alice, USDC(amount), c, blobs, nonce, dl, priv.address, chainId]);
    await (await priv.mint(alice, USDC(amount), c, blobs, nonce, dl, sig)).wait();
    return note;
  }

  it("mints: USDC pulled in, supply up, reserve backs it", async () => {
    await mint(1000);
    expect(await priv.totalSupply()).to.equal(USDC(1000));
    expect(await usdc.balanceOf(priv.address)).to.equal(USDC(1000));
    expect(await priv.reserveBacked()).to.equal(true);
  });

  it("shielded transfer conserves value and spends the input", async () => {
    const aliceNote = await mint(1000);
    const bobNote = notes.makeNote(bob, USDC(250));
    const change = notes.makeNote(alice, USDC(750));
    const nulls = [notes.nullifierOf(aliceNote)];
    const outs = [notes.commitmentOf(bobNote), notes.commitmentOf(change)];
    const blobs = ["b1", "b2"];
    const nonce = rand32();
    const dl = deadline();
    const sig = await sign(oracle,
      ["string", "bytes32[]", "bytes32[]", "string[]", "bytes32", "uint256", "address", "uint256"],
      ["TRANSFER", nulls, outs, blobs, nonce, dl, priv.address, chainId]);
    await (await priv.shieldedTransfer(nulls, outs, blobs, nonce, dl, sig)).wait();

    expect(await priv.totalSupply()).to.equal(USDC(1000)); // conserved
    expect(await priv.nullifiers(nulls[0])).to.equal(true);
    expect(await priv.commitments(outs[0])).to.equal(true);
  });

  it("redeem burns privUSD and releases USDC", async () => {
    const aliceNote = await mint(1000);
    const change = notes.makeNote(alice, USDC(600));
    const nulls = [notes.nullifierOf(aliceNote)];
    const outs = [notes.commitmentOf(change)];
    const blobs = ["b"];
    const nonce = rand32();
    const dl = deadline();
    const before = await usdc.balanceOf(bob);
    const sig = await sign(oracle,
      ["string", "bytes32[]", "bytes32[]", "string[]", "uint256", "address", "bytes32", "uint256", "address", "uint256"],
      ["REDEEM", nulls, outs, blobs, USDC(400), bob, nonce, dl, priv.address, chainId]);
    await (await priv.redeem(nulls, outs, blobs, USDC(400), bob, nonce, dl, sig)).wait();

    expect(await priv.totalSupply()).to.equal(USDC(600));
    expect((await usdc.balanceOf(bob)).sub(before)).to.equal(USDC(400));
  });

  it("rejects a forged signature (wrong signer)", async () => {
    const note = notes.makeNote(alice, USDC(100));
    const imposter = ethers.Wallet.createRandom();
    const nonce = rand32();
    const dl = deadline();
    const sig = await sign(imposter,
      ["string", "address", "uint256", "bytes32[]", "string[]", "bytes32", "uint256", "address", "uint256"],
      ["MINT", alice, USDC(100), [notes.commitmentOf(note)], ["blob"], nonce, dl, priv.address, chainId]);
    await expect(
      priv.mint(alice, USDC(100), [notes.commitmentOf(note)], ["blob"], nonce, dl, sig)
    ).to.be.revertedWithCustomError(priv, "InvalidOracleSignature");
  });

  it("rejects a replayed nonce", async () => {
    const note = notes.makeNote(alice, USDC(100));
    const c = [notes.commitmentOf(note)];
    const nonce = rand32();
    const dl = deadline();
    const sig = await sign(oracle,
      ["string", "address", "uint256", "bytes32[]", "string[]", "bytes32", "uint256", "address", "uint256"],
      ["MINT", alice, USDC(100), c, ["blob"], nonce, dl, priv.address, chainId]);
    await (await priv.mint(alice, USDC(100), c, ["blob"], nonce, dl, sig)).wait();
    await expect(
      priv.mint(alice, USDC(100), c, ["blob"], nonce, dl, sig)
    ).to.be.revertedWithCustomError(priv, "NonceAlreadyUsed");
  });

  it("rejects a fee-on-transfer reserve (balance-delta guard)", async () => {
    const fee = await (await ethers.getContractFactory("MockFeeUSDC")).deploy();
    await fee.deployed();
    await (await fee.mint(alice, USDC(10000))).wait();
    const privFee = await (await ethers.getContractFactory("PrivUSD")).deploy(fee.address, oracle.address);
    await privFee.deployed();
    await (await fee.approve(privFee.address, USDC(10000))).wait();

    const note = notes.makeNote(alice, USDC(1000));
    const c = [notes.commitmentOf(note)];
    const nonce = rand32();
    const dl = deadline();
    const sig = await sign(oracle,
      ["string", "address", "uint256", "bytes32[]", "string[]", "bytes32", "uint256", "address", "uint256"],
      ["MINT", alice, USDC(1000), c, ["blob"], nonce, dl, privFee.address, chainId]);
    // 1% fee means only 990 USDC arrive while the mint claims 1000 → reverts.
    await expect(
      privFee.mint(alice, USDC(1000), c, ["blob"], nonce, dl, sig)
    ).to.be.revertedWithCustomError(privFee, "ReserveDeltaMismatch");
  });

  it("rejects double-spend of a nullifier", async () => {
    const aliceNote = await mint(500);
    const out1 = notes.makeNote(alice, USDC(500));
    const nulls = [notes.nullifierOf(aliceNote)];
    const mkTransfer = async () => {
      const outs = [notes.commitmentOf(notes.makeNote(alice, USDC(500)))];
      const nonce = rand32();
      const dl = deadline();
      const sig = await sign(oracle,
        ["string", "bytes32[]", "bytes32[]", "string[]", "bytes32", "uint256", "address", "uint256"],
        ["TRANSFER", nulls, outs, ["b"], nonce, dl, priv.address, chainId]);
      return priv.shieldedTransfer(nulls, outs, ["b"], nonce, dl, sig);
    };
    await (await mkTransfer()).wait();
    await expect(mkTransfer()).to.be.revertedWithCustomError(priv, "NoteAlreadySpent");
  });
});
