import { expect } from "chai";
import { ethers } from "hardhat";
import { ZeroAddress, parseUnits } from "ethers";

const ONE_LITKEY = parseUnits("1", 18);
const TEN_LITKEY = parseUnits("10", 18);
const HUNDRED_LITKEY = parseUnits("100", 18);

describe("LitkeyPaymentGateway", () => {
  // ─── fixtures ────────────────────────────────────────────────────────────
  async function deployFixture() {
    const [deployer, treasury, alice, bob, carol] = await ethers.getSigners();

    const Mock = await ethers.getContractFactory("MockERC20");
    const litkey = await Mock.deploy();
    await litkey.waitForDeployment();

    const Gateway = await ethers.getContractFactory("LitkeyPaymentGateway");
    const gateway = await Gateway.deploy(
      await litkey.getAddress(),
      treasury.address,
    );
    await gateway.waitForDeployment();

    return { gateway, litkey, deployer, treasury, alice, bob, carol };
  }

  // ─── constructor ─────────────────────────────────────────────────────────
  describe("constructor", () => {
    it("stores litkey + treasury", async () => {
      const { gateway, litkey, treasury } = await deployFixture();
      expect(await gateway.litkey()).to.equal(await litkey.getAddress());
      expect(await gateway.treasury()).to.equal(treasury.address);
    });

    it("reverts when treasury is the zero address", async () => {
      const Mock = await ethers.getContractFactory("MockERC20");
      const litkey = await Mock.deploy();

      const Gateway = await ethers.getContractFactory("LitkeyPaymentGateway");
      await expect(
        Gateway.deploy(await litkey.getAddress(), ZeroAddress),
      ).to.be.revertedWithCustomError(Gateway, "InvalidTreasury");
    });
  });

  // ─── pay() ───────────────────────────────────────────────────────────────
  describe("pay", () => {
    it("transfers LITKEY to treasury and emits Payment", async () => {
      const { gateway, litkey, treasury, alice, bob } = await deployFixture();

      // Alice has LITKEY and approves the gateway.
      await litkey.mint(alice.address, HUNDRED_LITKEY);
      await litkey
        .connect(alice)
        .approve(await gateway.getAddress(), TEN_LITKEY);

      // She pays 10 LITKEY to credit bob's Lit account.
      await expect(gateway.connect(alice).pay(TEN_LITKEY, bob.address))
        .to.emit(gateway, "Payment")
        .withArgs(bob.address, alice.address, TEN_LITKEY);

      expect(await litkey.balanceOf(treasury.address)).to.equal(TEN_LITKEY);
      expect(await litkey.balanceOf(alice.address)).to.equal(
        HUNDRED_LITKEY - TEN_LITKEY,
      );
      expect(await litkey.balanceOf(await gateway.getAddress())).to.equal(0n);
    });

    it("reverts when wallet is the zero address", async () => {
      const { gateway, litkey, alice } = await deployFixture();
      await litkey.mint(alice.address, TEN_LITKEY);
      await litkey
        .connect(alice)
        .approve(await gateway.getAddress(), TEN_LITKEY);

      await expect(
        gateway.connect(alice).pay(TEN_LITKEY, ZeroAddress),
      ).to.be.revertedWithCustomError(gateway, "InvalidWallet");
    });

    it("reverts when amount is zero", async () => {
      const { gateway, alice, bob } = await deployFixture();
      await expect(
        gateway.connect(alice).pay(0n, bob.address),
      ).to.be.revertedWithCustomError(gateway, "InvalidAmount");
    });

    it("reverts when the payer hasn't approved the gateway", async () => {
      const { gateway, litkey, alice, bob } = await deployFixture();
      await litkey.mint(alice.address, TEN_LITKEY);
      // No approve() call.

      await expect(
        gateway.connect(alice).pay(TEN_LITKEY, bob.address),
      ).to.be.revertedWithCustomError(litkey, "ERC20InsufficientAllowance");
    });

    it("reverts when payer's balance is insufficient", async () => {
      const { gateway, litkey, alice, bob } = await deployFixture();
      await litkey.mint(alice.address, ONE_LITKEY);
      await litkey
        .connect(alice)
        .approve(await gateway.getAddress(), TEN_LITKEY);

      await expect(
        gateway.connect(alice).pay(TEN_LITKEY, bob.address),
      ).to.be.revertedWithCustomError(litkey, "ERC20InsufficientBalance");
    });

    it("supports the same payer paying multiple times", async () => {
      const { gateway, litkey, treasury, alice, bob } = await deployFixture();
      await litkey.mint(alice.address, HUNDRED_LITKEY);
      await litkey
        .connect(alice)
        .approve(await gateway.getAddress(), HUNDRED_LITKEY);

      await gateway.connect(alice).pay(TEN_LITKEY, bob.address);
      await gateway.connect(alice).pay(ONE_LITKEY, bob.address);

      expect(await litkey.balanceOf(treasury.address)).to.equal(
        TEN_LITKEY + ONE_LITKEY,
      );
    });

    it("attributes concurrent payments from different payers correctly", async () => {
      const { gateway, litkey, treasury, alice, bob, carol } =
        await deployFixture();

      await litkey.mint(alice.address, HUNDRED_LITKEY);
      await litkey.mint(bob.address, HUNDRED_LITKEY);
      await litkey
        .connect(alice)
        .approve(await gateway.getAddress(), HUNDRED_LITKEY);
      await litkey
        .connect(bob)
        .approve(await gateway.getAddress(), HUNDRED_LITKEY);

      // Alice credits carol; bob credits himself.
      await expect(gateway.connect(alice).pay(TEN_LITKEY, carol.address))
        .to.emit(gateway, "Payment")
        .withArgs(carol.address, alice.address, TEN_LITKEY);
      await expect(gateway.connect(bob).pay(ONE_LITKEY, bob.address))
        .to.emit(gateway, "Payment")
        .withArgs(bob.address, bob.address, ONE_LITKEY);

      expect(await litkey.balanceOf(treasury.address)).to.equal(
        TEN_LITKEY + ONE_LITKEY,
      );
    });

    it("lets users credit a wallet different from the payer (cross-wallet payment)", async () => {
      const { gateway, litkey, alice, bob } = await deployFixture();
      await litkey.mint(alice.address, TEN_LITKEY);
      await litkey
        .connect(alice)
        .approve(await gateway.getAddress(), TEN_LITKEY);

      // Alice (paying wallet) credits bob (Lit account wallet).
      await expect(gateway.connect(alice).pay(TEN_LITKEY, bob.address))
        .to.emit(gateway, "Payment")
        .withArgs(bob.address, alice.address, TEN_LITKEY);
    });
  });

  // ─── event indexing ──────────────────────────────────────────────────────
  describe("event filtering", () => {
    it("filters Payment by indexed wallet", async () => {
      const { gateway, litkey, alice, bob, carol } = await deployFixture();
      await litkey.mint(alice.address, HUNDRED_LITKEY);
      await litkey
        .connect(alice)
        .approve(await gateway.getAddress(), HUNDRED_LITKEY);

      await gateway.connect(alice).pay(TEN_LITKEY, bob.address);
      await gateway.connect(alice).pay(ONE_LITKEY, carol.address);
      await gateway.connect(alice).pay(ONE_LITKEY, bob.address);

      // Filter for payments crediting bob — should see two.
      const filter = gateway.filters.Payment(bob.address);
      const events = await gateway.queryFilter(filter);
      expect(events).to.have.length(2);
      expect(events[0].args.amount).to.equal(TEN_LITKEY);
      expect(events[1].args.amount).to.equal(ONE_LITKEY);
      for (const e of events) {
        expect(e.args.wallet).to.equal(bob.address);
        expect(e.args.payer).to.equal(alice.address);
      }
    });

    it("filters Payment by indexed payer", async () => {
      const { gateway, litkey, alice, bob, carol } = await deployFixture();
      await litkey.mint(alice.address, HUNDRED_LITKEY);
      await litkey.mint(bob.address, HUNDRED_LITKEY);
      await litkey
        .connect(alice)
        .approve(await gateway.getAddress(), HUNDRED_LITKEY);
      await litkey
        .connect(bob)
        .approve(await gateway.getAddress(), HUNDRED_LITKEY);

      await gateway.connect(alice).pay(ONE_LITKEY, carol.address);
      await gateway.connect(bob).pay(ONE_LITKEY, carol.address);
      await gateway.connect(alice).pay(ONE_LITKEY, carol.address);

      // Filter for alice as payer — should see two.
      const filter = gateway.filters.Payment(undefined, alice.address);
      const events = await gateway.queryFilter(filter);
      expect(events).to.have.length(2);
      for (const e of events) {
        expect(e.args.payer).to.equal(alice.address);
      }
    });
  });
});
