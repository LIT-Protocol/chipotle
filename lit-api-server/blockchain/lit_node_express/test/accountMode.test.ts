import { expect } from "chai";
import { ethers } from "hardhat";
import { deployAccountConfig } from "./helpers/deployDiamond";
const { readAccountManaged } = require("../examples/read-account-managed.cjs");

// Exercise the exact integrator snippet against real diamond storage via JSON-RPC.
describe("Account mode getter and storage fallback", () => {
  const MASTER = 101n;
  const USAGE = 202n;
  let diamond: string;
  let writes: any;
  let views: any;
  let owner: any;
  let admin: any;
  let stranger: any;

  beforeEach(async () => {
    [owner, admin, stranger] = await ethers.getSigners();
    diamond = await deployAccountConfig(owner);
    writes = await ethers.getContractAt("WritesFacet", diamond, owner);
    views = await ethers.getContractAt("ViewsFacet", diamond, stranger);
    const config = await ethers.getContractAt("APIConfigFacet", diamond, owner);
    await config.setApiPayers([await owner.getAddress()]);
    await writes.newAccount(MASTER, true, "account", "", await admin.getAddress());
    await writes.setUsageApiKey(
      MASTER, USAGE, 1, 0, "expired usage", "", false, false, false, [], [], [], [],
    );
  });

  async function assertMode(hash: bigint, managed: boolean) {
    expect(await views.isAccountManaged(hash)).to.equal(managed);
    expect(await readAccountManaged(ethers.provider, diamond, hash)).to.equal(managed);
  }

  async function assertMissing(hash: bigint) {
    await expect(views.isAccountManaged(hash))
      .to.be.revertedWithCustomError(views, "AccountDoesNotExist").withArgs(hash);
    let error: unknown;
    try { await readAccountManaged(ethers.provider, diamond, hash); }
    catch (e) { error = e; }
    expect(error).to.be.instanceOf(Error);
    expect((error as Error).message).to.equal("Unknown or removed account/key hash");
  }

  it("reads managed master and expired usage keys from an unrelated caller", async () => {
    await assertMode(MASTER, true);
    await assertMode(USAGE, true);
    expect(await views.accountExistsAndIsMutable(MASTER)).to.equal(false);
  });

  it("reads a newly created ChainSecured account", async () => {
    await writes.connect(stranger).newChainSecuredAccount("chain", "");
    const hash = BigInt(ethers.solidityPackedKeccak256(["address"], [await stranger.getAddress()]));
    await assertMode(hash, false);
  });

  it("tracks conversion through master, usage and admin alias; supports historical reads", async () => {
    const before = await ethers.provider.send("eth_blockNumber", []);
    await writes.convertToChainSecuredAccount(MASTER, await stranger.getAddress());
    const alias = BigInt(ethers.solidityPackedKeccak256(["address"], [await stranger.getAddress()]));
    await assertMode(MASTER, false);
    await assertMode(USAGE, false);
    await assertMode(alias, false);
    expect(await readAccountManaged(ethers.provider, diamond, MASTER, before)).to.equal(true);
  });

  it("rejects unknown and removed hashes instead of reporting ChainSecured", async () => {
    await assertMissing(0n);
    await assertMissing(999n);
    await writes.removeUsageApiKey(MASTER, USAGE);
    await assertMissing(USAGE);
  });

  it("does not depend on payer membership, even when a payer owns a ChainSecured account", async () => {
    await writes.connect(stranger).newChainSecuredAccount("chain", "");
    const hash = BigInt(ethers.solidityPackedKeccak256(["address"], [await stranger.getAddress()]));
    const config = await ethers.getContractAt("APIConfigFacet", diamond, owner);
    await config.setApiPayers([await stranger.getAddress()]);
    expect(await views.accountExistsAndIsMutable(hash)).to.equal(true);
    await assertMode(hash, false);
    await config.setApiPayers([]);
    await assertMode(MASTER, true);
    await assertMode(hash, false);
  });
});
