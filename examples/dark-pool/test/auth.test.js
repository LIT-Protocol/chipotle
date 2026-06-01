// Unit tests for the order authentication inside action/matchEpoch.js.
// Extracts the pure functions from the action source (single source of truth)
// and feeds it real ethers, so the signature logic under test is exactly what
// runs in the enclave.

const fs = require("fs");
const path = require("path");
const { expect } = require("chai");
const { ethers } = require("hardhat");

const src = fs.readFileSync(path.join(__dirname, "..", "action", "matchEpoch.js"), "utf8");
const factory = new Function(
  "module",
  "exports",
  "ethers",
  "Lit",
  "fetch",
  "URL",
  src + "\nmodule.exports = { authenticateOrders, runAuction, ration };"
);
const mod = { exports: {} };
factory(mod, mod.exports, ethers, {}, () => {}, global.URL);
const { authenticateOrders } = mod.exports;

const CTX = {
  chainId: 84532,
  settlement: "0x1111111111111111111111111111111111111111",
  epoch: 1,
  pair: "BASE/QUOTE",
};

let idc = 0;

// Build an order signed exactly the way submitOrder.js does.
async function signedOrder(wallet, overrides = {}) {
  const o = {
    id: ++idc,
    side: "buy",
    limitPrice: 100n * 10n ** 18n,
    quantity: 5n * 10n ** 18n,
    nonce: String(++idc),
    trader: wallet.address,
    ...overrides,
  };
  const ctx = overrides._signCtx || CTX;
  const pairHash = ethers.utils.keccak256(ethers.utils.toUtf8Bytes(ctx.pair));
  const digest = ethers.utils.keccak256(
    ethers.utils.defaultAbiCoder.encode(
      ["uint256", "address", "uint256", "bytes32", "bool", "uint256", "uint256", "uint256"],
      [String(ctx.chainId), ctx.settlement, String(ctx.epoch), pairHash, o.side === "buy", o.limitPrice.toString(), o.quantity.toString(), String(o.nonce)]
    )
  );
  o.sig = overrides.sig || (await wallet.signMessage(ethers.utils.arrayify(digest)));
  delete o._signCtx;
  return o;
}

describe("authenticateOrders", function () {
  it("accepts a correctly signed order", async function () {
    const w = ethers.Wallet.createRandom();
    const { accepted, rejected } = authenticateOrders([await signedOrder(w)], CTX);
    expect(accepted).to.have.length(1);
    expect(rejected).to.have.length(0);
  });

  it("rejects an order whose signature is by someone other than `trader`", async function () {
    const attacker = ethers.Wallet.createRandom();
    const victim = ethers.Wallet.createRandom();
    // attacker signs, but claims the victim as trader (the forge-an-order attack)
    const o = await signedOrder(attacker, { trader: victim.address });
    const { accepted, rejected } = authenticateOrders([o], CTX);
    expect(accepted).to.have.length(0);
    expect(rejected[0].reason).to.match(/signature does not match trader/);
  });

  it("rejects a duplicate (trader, nonce) replay", async function () {
    const w = ethers.Wallet.createRandom();
    const o = await signedOrder(w, { nonce: "777" });
    const dup = { ...o, id: 999 };
    const { accepted, rejected } = authenticateOrders([o, dup], CTX);
    expect(accepted).to.have.length(1);
    expect(rejected[0].reason).to.match(/duplicate/);
  });

  it("rejects out-of-range quantity", async function () {
    const w = ethers.Wallet.createRandom();
    const o = await signedOrder(w, { quantity: 2n ** 128n });
    const { accepted, rejected } = authenticateOrders([o], CTX);
    expect(accepted).to.have.length(0);
    expect(rejected[0].reason).to.match(/quantity out of range/);
  });

  it("rejects a missing signature", async function () {
    const w = ethers.Wallet.createRandom();
    const o = await signedOrder(w);
    delete o.sig;
    const { rejected } = authenticateOrders([o], CTX);
    expect(rejected[0].reason).to.match(/missing nonce\/signature/);
  });

  it("rejects an order signed for a different epoch (binding)", async function () {
    const w = ethers.Wallet.createRandom();
    // sign bound to epoch 2, authenticate in epoch 1
    const o = await signedOrder(w, { _signCtx: { ...CTX, epoch: 2 } });
    const { accepted, rejected } = authenticateOrders([o], CTX);
    expect(accepted).to.have.length(0);
    expect(rejected[0].reason).to.match(/signature does not match trader/);
  });
});
