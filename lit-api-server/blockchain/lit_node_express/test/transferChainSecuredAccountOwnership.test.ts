import { expect } from "chai";
import { ethers } from "hardhat";
import type { Signer } from "ethers";
import { deployAccountConfig } from "./helpers/deployDiamond";

function apiKeyHashFor(address: string): bigint {
  return BigInt(ethers.keccak256(ethers.solidityPacked(["address"], [address])));
}

describe("WritesFacet.transferChainSecuredAccountOwnership", () => {
  let diamondAddress: string;
  let owner: Signer;
  let admin: Signer;
  let newAdmin: Signer;
  let stranger: Signer;
  let apiPayer: Signer;

  beforeEach(async () => {
    [owner, admin, newAdmin, stranger, apiPayer] = await ethers.getSigners();
    diamondAddress = await deployAccountConfig(owner);
  });

  async function writes(signer: Signer) {
    return ethers.getContractAt("WritesFacet", diamondAddress, signer);
  }
  async function views(signer: Signer) {
    return ethers.getContractAt("ViewsFacet", diamondAddress, signer);
  }
  async function apiConfig(signer: Signer) {
    return ethers.getContractAt("APIConfigFacet", diamondAddress, signer);
  }

  async function createChainSecuredAs(signer: Signer) {
    const w = await writes(signer);
    await (await w.newChainSecuredAccount("acct", "desc")).wait();
    return apiKeyHashFor(await signer.getAddress());
  }

  it("transfers ownership to a new wallet and preserves apiKeyHash + billing", async () => {
    const apiKeyHash = await createChainSecuredAs(admin);
    const adminAddress = await admin.getAddress();
    const newAdminAddress = await newAdmin.getAddress();

    const viewsAsAdmin = await views(admin);
    expect(await viewsAsAdmin.getAccountWalletAddress(apiKeyHash)).to.equal(
      adminAddress,
    );
    expect(await viewsAsAdmin.getBillingWalletAddress(apiKeyHash)).to.equal(
      adminAddress,
    );

    const w = await writes(admin);
    await expect(
      w.transferChainSecuredAccountOwnership(apiKeyHash, newAdminAddress),
    )
      .to.emit(w, "ChainSecuredAccountOwnershipTransferred")
      .withArgs(apiKeyHash, adminAddress, newAdminAddress);

    const viewsAsNewAdmin = await views(newAdmin);
    expect(await viewsAsNewAdmin.getAccountWalletAddress(apiKeyHash)).to.equal(
      newAdminAddress,
    );
    // billing wallet intentionally left with the previous admin (per CPL-324
    // design: transfer admin only, not billing).
    expect(await viewsAsNewAdmin.getBillingWalletAddress(apiKeyHash)).to.equal(
      adminAddress,
    );

    // The new admin can now resolve into the account using their own
    // keccak256(address) — confirming the new mapping was written.
    const newAdminHash = apiKeyHashFor(newAdminAddress);
    expect(await viewsAsNewAdmin.accountExistsAndIsMutable(newAdminHash)).to.equal(
      true,
    );
  });

  it("reverts when a non-admin caller tries to transfer", async () => {
    const apiKeyHash = await createChainSecuredAs(admin);
    const w = await writes(stranger);
    await expect(
      w.transferChainSecuredAccountOwnership(
        apiKeyHash,
        await newAdmin.getAddress(),
      ),
    )
      .to.be.revertedWithCustomError(w, "NoAccountAccess")
      .withArgs(apiKeyHash, await stranger.getAddress());
  });

  it("reverts when an api_payer tries to transfer a ChainSecured account", async () => {
    const apiKeyHash = await createChainSecuredAs(admin);
    // Make `apiPayer` an api_payer. Only the diamond owner can do this.
    const cfg = await apiConfig(owner);
    await (await cfg.setApiPayers([await apiPayer.getAddress()])).wait();
    const w = await writes(apiPayer);
    await expect(
      w.transferChainSecuredAccountOwnership(
        apiKeyHash,
        await newAdmin.getAddress(),
      ),
    )
      .to.be.revertedWithCustomError(w, "NoAccountAccess")
      .withArgs(apiKeyHash, await apiPayer.getAddress());
  });

  it("reverts when newAdminWalletAddress is the zero address", async () => {
    const apiKeyHash = await createChainSecuredAs(admin);
    const w = await writes(admin);
    await expect(
      w.transferChainSecuredAccountOwnership(apiKeyHash, ethers.ZeroAddress),
    )
      .to.be.revertedWithCustomError(w, "InvalidRequest")
      .withArgs("newAdminWalletAddress must be non-zero");
  });

  it("reverts when newAdminWalletAddress equals the current admin", async () => {
    const apiKeyHash = await createChainSecuredAs(admin);
    const w = await writes(admin);
    await expect(
      w.transferChainSecuredAccountOwnership(
        apiKeyHash,
        await admin.getAddress(),
      ),
    )
      .to.be.revertedWithCustomError(w, "InvalidRequest")
      .withArgs("newAdminWalletAddress must differ from current admin");
  });

  it("reverts when the account does not exist", async () => {
    const bogusHash = apiKeyHashFor(await stranger.getAddress());
    const w = await writes(admin);
    await expect(
      w.transferChainSecuredAccountOwnership(
        bogusHash,
        await newAdmin.getAddress(),
      ),
    )
      .to.be.revertedWithCustomError(w, "AccountDoesNotExist")
      .withArgs(bogusHash);
  });

  it("reverts when the new admin already owns a ChainSecured account", async () => {
    const apiKeyHash = await createChainSecuredAs(admin);
    // newAdmin already has their own ChainSecured account.
    const conflictingHash = await createChainSecuredAs(newAdmin);
    const w = await writes(admin);
    await expect(
      w.transferChainSecuredAccountOwnership(
        apiKeyHash,
        await newAdmin.getAddress(),
      ),
    )
      .to.be.revertedWithCustomError(w, "AccountAlreadyExists")
      .withArgs(conflictingHash);
  });

  it("reverts on a managed (API-mode) account; convert first", async () => {
    // Owner creates a managed account on behalf of a fake apiKeyHash.
    const managedHash = ethers.toBigInt(ethers.id("managed-account-1"));
    const w = await writes(owner);
    await (
      await w.newAccount(
        managedHash,
        true,
        "managed",
        "desc",
        await admin.getAddress(),
      )
    ).wait();
    const wAdmin = await writes(admin);
    await expect(
      wAdmin.transferChainSecuredAccountOwnership(
        managedHash,
        await newAdmin.getAddress(),
      ),
    )
      .to.be.revertedWithCustomError(wAdmin, "InvalidRequest")
      .withArgs(
        "Account is not ChainSecured; use convertToChainSecuredAccount instead.",
      );
  });
});
