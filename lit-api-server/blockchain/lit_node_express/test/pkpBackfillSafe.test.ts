import { expect } from "chai";
import { ethers, artifacts } from "hardhat";
import {
  BACKFILL_SELECTOR,
  buildSafeBatch,
  buildSafeTx,
  chunk,
  decodeMultiSend,
  decodeTransactionsToPairs,
  diamondIface,
  encodeBackfillCall,
  multiSendIface,
  Pair,
} from "../tasks/lib/pkp-owners";

function pair(n: number): Pair {
  return {
    pkpId: ethers.getAddress(
      "0x" + BigInt(n + 0x1000).toString(16).padStart(40, "0")
    ),
    masterHash: BigInt(n) * 7n + 1n,
  };
}

/** Manually pack Safe MultiSend bytes for a list of {to,data} calls. */
function packMultiSend(calls: { to: string; data: string }[]): string {
  let packed = "0x";
  for (const c of calls) {
    const data = ethers.getBytes(c.data);
    packed += ethers.solidityPacked(
      ["uint8", "address", "uint256", "uint256", "bytes"],
      [0, c.to, 0, data.length, c.data]
    ).slice(2);
  }
  return multiSendIface.encodeFunctionData("multiSend", [packed]);
}

describe("pkp-backfill-safe lib", () => {
  const diamond = ethers.getAddress(
    "0xaAaAA9120fE271F653cfDb6bf400dB93D2DEa7Aa"
  );

  it("DIAMOND_ABI selectors match the compiled facets (no ABI drift)", async () => {
    const writes = await artifacts.readArtifact("WritesFacet");
    const views = await artifacts.readArtifact("ViewsFacet");
    const writesIface = new ethers.Interface(writes.abi);
    const viewsIface = new ethers.Interface(views.abi);

    expect(BACKFILL_SELECTOR).to.equal(
      writesIface.getFunction("backfillPkpOwners")!.selector
    );
    expect(diamondIface.getFunction("getPkpOwnerMaster")!.selector).to.equal(
      viewsIface.getFunction("getPkpOwnerMaster")!.selector
    );
    // The event topic the scan filters on must match the contract's event.
    expect(diamondIface.getEvent("WalletDerivationRegistered")!.topicHash).to.equal(
      writesIface.getEvent("WalletDerivationRegistered")!.topicHash
    );
  });

  it("encodes and decodes a backfill call round-trip", () => {
    const pairs = [pair(1), pair(2), pair(3)];
    const data = encodeBackfillCall(pairs);
    const tx = buildSafeTx(diamond, data);
    expect(tx.to).to.equal(diamond);
    expect(tx.value).to.equal("0");

    const { pairs: decoded, targets } = decodeTransactionsToPairs([tx]);
    expect(decoded).to.have.length(3);
    expect([...targets]).to.deep.equal([diamond.toLowerCase()]);
    for (let i = 0; i < pairs.length; i++) {
      expect(decoded[i].pkpId).to.equal(pairs[i].pkpId);
      expect(decoded[i].masterHash).to.equal(pairs[i].masterHash);
    }
  });

  it("decodes multiple calls in one Safe batch file", () => {
    const c1 = [pair(1), pair(2)];
    const c2 = [pair(3), pair(4), pair(5)];
    const batch = buildSafeBatch(8453, 123, "t", "d", [
      buildSafeTx(diamond, encodeBackfillCall(c1)),
      buildSafeTx(diamond, encodeBackfillCall(c2)),
    ]);
    const { pairs } = decodeTransactionsToPairs(
      batch.transactions.map((t) => ({ to: t.to, data: t.data }))
    );
    expect(pairs.map((p) => p.pkpId)).to.deep.equal(
      [...c1, ...c2].map((p) => p.pkpId)
    );
  });

  it("unwraps a MultiSend-bundled tx into backfill pairs", () => {
    const c1 = [pair(10), pair(11)];
    const c2 = [pair(12)];
    const bundle = packMultiSend([
      { to: diamond, data: encodeBackfillCall(c1) },
      { to: diamond, data: encodeBackfillCall(c2) },
    ]);
    const inner = decodeMultiSend(bundle);
    expect(inner).to.have.length(2);
    expect(inner[0].to).to.equal(diamond);

    const { pairs, targets } = decodeTransactionsToPairs([
      { to: "0x40A2aCCbd92BCA938b02010E17A5b8929b49130D", data: bundle },
    ]);
    expect(pairs.map((p) => p.masterHash)).to.deep.equal(
      [...c1, ...c2].map((p) => p.masterHash)
    );
    expect([...targets]).to.deep.equal([diamond.toLowerCase()]);
  });

  it("rejects an unexpected selector", () => {
    const bogus = "0xdeadbeef" + "00".repeat(32);
    expect(() =>
      decodeTransactionsToPairs([{ to: diamond, data: bogus }])
    ).to.throw(/Unexpected function selector/);
  });

  it("chunk() splits evenly with a final short batch", () => {
    const arr = Array.from({ length: 2500 }, (_, i) => i);
    const batches = chunk(arr, 1000);
    expect(batches.map((b) => b.length)).to.deep.equal([1000, 1000, 500]);
  });
});
