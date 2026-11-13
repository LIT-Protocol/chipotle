// Shared by k6 and the offline verifier. Never serialize arbitrary action output.
export function publicResult(op, body) {
  const bad = () => {
    throw new Error(`${op}: malformed or failed action result`);
  };
  const hex = (value, bytes) =>
    typeof value === "string" &&
    new RegExp(`^0x[0-9a-f]{${bytes * 2}}$`, "i").test(value);
  if (body?.has_error !== false) return bad();
  const r = body.response;
  if (op === "hello") {
    if (r !== "benchmark-ok") return bad();
    return { has_error: false, response: r };
  }
  if (!r || !hex(r.address, 20)) return bad();
  if (op === "action_sign" || op === "pkp_sign") {
    if (!hex(r.signature, 65)) return bad();
    return {
      has_error: false,
      response: { address: r.address, signature: r.signature },
    };
  }
  if (
    !["relay_wallet", "relay_sign"].includes(op) ||
    r.vmType !== "ethereum-vm"
  )
    return bad();
  const result = { address: r.address, vmType: r.vmType };
  if (op === "relay_sign") {
    if (
      !hex(r.withdrawRequestHash, 32) ||
      !Array.isArray(r.results) ||
      r.results.length !== 1
    )
      return bad();
    result.withdrawRequestHash = r.withdrawRequestHash;
    result.results = r.results.map((s) => {
      if (!hex(s.hash, 32) || !hex(s.signature, 65)) return bad();
      return { hash: s.hash, signature: s.signature };
    });
  }
  return { has_error: false, response: result };
}
