// Unit tests for the pure uniform-price auction inside action/matchEpoch.js.
//
// We load the action source and extract its top-level pure functions
// (runAuction, ration) without executing main(), so there is a single source
// of truth: the code that actually runs in the enclave is the code under test.
// main() references Lit/ethers/fetch, but it is never called here, so the
// stubbed globals are never touched.

const fs = require("fs");
const path = require("path");
const { expect } = require("chai");

const src = fs.readFileSync(path.join(__dirname, "..", "action", "matchEpoch.js"), "utf8");
const factory = new Function(
  "module",
  "exports",
  "ethers",
  "Lit",
  "fetch",
  "URL",
  src + "\nmodule.exports = { runAuction, ration };"
);
const mod = { exports: {} };
factory(mod, mod.exports, {}, {}, () => {}, global.URL);
const { runAuction } = mod.exports;

let nextId = 0;
function order(side, limitPrice, quantity) {
  return {
    id: nextId++,
    side,
    limitPrice: BigInt(limitPrice),
    quantity: BigInt(quantity),
    trader: "0x" + String(nextId).padStart(40, "0"),
  };
}
const sum = (fills, isBuy) =>
  fills.filter((f) => f.isBuy === isBuy).reduce((a, f) => a + f.quantity, 0n);

beforeEach(() => {
  nextId = 1;
});

describe("runAuction", function () {
  it("clears a simple crossing pair at the limit price", function () {
    const { clearingPx, fills } = runAuction([order("buy", 100, 5), order("sell", 100, 5)]);
    expect(clearingPx).to.equal(100n);
    expect(sum(fills, true)).to.equal(5n);
    expect(sum(fills, false)).to.equal(5n);
  });

  it("conserves base when rationing the long side (rounding dust)", function () {
    // buys total 10, sells total 7 -> V=7, buys rationed. 3*7/10 etc. leaves dust.
    const { clearingPx, fills } = runAuction([
      order("buy", 100, 3),
      order("buy", 100, 3),
      order("buy", 100, 4),
      order("sell", 100, 7),
    ]);
    expect(clearingPx).to.equal(100n);
    expect(sum(fills, true)).to.equal(7n); // buys rationed to exactly the matched volume
    expect(sum(fills, false)).to.equal(7n); // sells fully filled
    expect(sum(fills, true)).to.equal(sum(fills, false)); // conservation
  });

  it("returns no fills when the book doesn't cross", function () {
    const { clearingPx, fills } = runAuction([order("buy", 99, 10), order("sell", 100, 10)]);
    expect(clearingPx).to.equal(0n);
    expect(fills).to.have.length(0);
  });

  it("picks the volume-maximising price; ties break to the lower price", function () {
    const { clearingPx, fills } = runAuction([order("buy", 105, 10), order("sell", 95, 10)]);
    expect(clearingPx).to.equal(95n);
    expect(sum(fills, true)).to.equal(10n);
    expect(sum(fills, false)).to.equal(10n);
  });

  it("conserves base across a messy book (fuzz-ish, fixed seed)", function () {
    // Deterministic pseudo-random book; the invariant must hold every time.
    const orders = [];
    let seed = 12345;
    const rnd = (n) => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) % n);
    for (let i = 0; i < 40; i++) {
      const side = rnd(2) === 0 ? "buy" : "sell";
      const price = 90 + rnd(20); // 90..109
      const qty = 1 + rnd(13);
      orders.push(order(side, price, qty));
    }
    const { fills } = runAuction(orders);
    expect(sum(fills, true)).to.equal(sum(fills, false)); // base bought == base sold, always
  });

  it("is deterministic (same input -> same fills)", function () {
    const mk = () => [order("buy", 100, 3), order("buy", 100, 3), order("buy", 100, 4), order("sell", 100, 7)];
    nextId = 1;
    const a = runAuction(mk());
    nextId = 1;
    const b = runAuction(mk());
    expect(JSON.stringify(a, (k, v) => (typeof v === "bigint" ? v.toString() : v))).to.equal(
      JSON.stringify(b, (k, v) => (typeof v === "bigint" ? v.toString() : v))
    );
  });
});
