import { recoverAddress, verifyMessage } from "ethers";
export const MESSAGE = "Chipotle benchmark: signature only, not a transaction";
export const OPERATIONS = [
  "hello",
  "action_sign",
  "pkp_sign",
  "relay_wallet",
  "relay_sign",
];
export function stats(values) {
  const s = values.toSorted((a, b) => a - b),
    n = s.length;
  if (!n) return { count: 0, medianMs: null, p95Ms: null };
  return {
    count: n,
    medianMs: (s[Math.floor((n - 1) / 2)] + s[Math.floor(n / 2)]) / 2,
    p95Ms: s[Math.ceil(n * 0.95) - 1],
  };
}
const same = (a, b) =>
  typeof a === "string" &&
  typeof b === "string" &&
  a.toLowerCase() === b.toLowerCase();
export function validate(op, body, expected, params) {
  if (!body || body.has_error !== false)
    throw new Error(`${op}: action error or malformed envelope`);
  const r = body.response;
  if (op === "hello") {
    if (r !== "benchmark-ok") throw new Error("hello: wrong result");
    return;
  }
  if (op === "action_sign" || op === "pkp_sign") {
    if (
      !same(verifyMessage(MESSAGE, r?.signature), r?.address) ||
      (expected && !same(r.address, expected))
    )
      throw new Error(`${op}: signer mismatch`);
    return r.address;
  }
  if (
    r?.vmType !== "ethereum-vm" ||
    !/^0x[0-9a-f]{40}$/i.test(r.address) ||
    (expected && !same(r.address, expected))
  )
    throw new Error(`${op}: wallet mismatch`);
  if (op === "relay_sign") {
    if (
      r.withdrawRequestHash !== params.attestation.withdrawRequestHash ||
      !Array.isArray(r.results) ||
      r.results.length !== params.attestation.hashesToSign.length
    )
      throw new Error("relay_sign: attestation mismatch");
    r.results.forEach((s, i) => {
      if (
        s.hash !== params.attestation.hashesToSign[i] ||
        !same(recoverAddress(s.hash, s.signature), r.address)
      )
        throw new Error("relay_sign: invalid signature");
    });
  }
  return r.address;
}
export function summarize(samples) {
  return Object.fromEntries(
    OPERATIONS.map((op) => [
      op,
      stats(
        samples
          .filter((s) => s.operation === op && !s.warmup && s.ok)
          .map((s) => s.clientMs),
      ),
    ]),
  );
}
