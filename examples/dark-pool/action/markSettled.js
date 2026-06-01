// Lit Action: after an epoch settles on-chain, flip its orders + epoch row to
// settled. Runs as a pinned action so the orchestrator never holds raw DB
// credentials — it decrypts the connection string inside the enclave.
//
// js_params:
//   pkpId, encryptedDbUrl, ids[], epoch, pair, clearingPx, txHash

async function main({ pkpId, encryptedDbUrl, ids, epoch, pair, clearingPx, txHash }) {
  const dbUrl = await Lit.Actions.Decrypt({ pkpId, ciphertext: encryptedDbUrl });
  const host = new URL(dbUrl).host;
  async function q(query, params) {
    const res = await fetch(`https://${host}/sql`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Neon-Connection-String": dbUrl,
        "Neon-Array-Mode": "true",
      },
      body: JSON.stringify({ query, params: params || [] }),
    });
    if (!res.ok) throw new Error(`neon sql request failed (status ${res.status})`);
    return res.json();
  }

  if (ids && ids.length) {
    await q("update orders set settled = true where id = any($1)", [ids]);
  }
  await q(
    "insert into epochs (epoch, pair, status, clearing_px, settled_tx, closed_at) " +
      "values ($1, $2, 'settled', $3, $4, now()) " +
      "on conflict (epoch) do update set status = 'settled', clearing_px = $3, settled_tx = $4, closed_at = now()",
    [epoch, pair, clearingPx, txHash]
  );
  return { ok: true, settled: (ids && ids.length) || 0 };
}
