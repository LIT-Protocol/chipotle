// Measure the relayed session-blob size at every round, raw and gzipped,
// against the 1 MB action response limit.
import { initSync, KeygenSession, SignSession, Message } from "./dkls-wasm-ll-web.js";
const rawWasm = await Deno.readFile(new URL("./dkls-wasm-ll-web_bg.wasm", import.meta.url));
initSync(rawWasm);

const others = (m: Message[], p: number) => m.filter((x) => x.from_id != p).map((x) => x.clone());
const toParty = (m: Message[], p: number) => m.filter((x) => x.to_id == p).map((x) => x.clone());

async function gz(b: Uint8Array): Promise<number> {
  const cs = new CompressionStream("gzip");
  const buf = await new Response(new Blob([b]).stream().pipeThrough(cs)).arrayBuffer();
  return buf.byteLength;
}
const k = (n: number) => (n / 1024).toFixed(1) + "KB";
async function report(label: string, b: Uint8Array) {
  const g = await gz(b);
  const flag = g > 1024 * 1024 ? "  ⚠️ >1MB even gzipped" : "";
  console.log(`  ${label.padEnd(22)} raw ${k(b.length).padStart(8)}  gzip ${k(g).padStart(8)}${flag}`);
}

console.log("DKG action-session blob per round:");
let u = new KeygenSession(2, 2, 0), a = new KeygenSession(2, 2, 1);
const m1u = u.createFirstMessage(), m1a = a.createFirstMessage();
await report("after createFirst", a.toBytes());
let m2u = u.handleMessages(others([m1u, m1a], 0)), m2a = a.handleMessages(others([m1u, m1a], 1));
await report("after round1", a.toBytes());
const ccU = u.calculateChainCodeCommitment(), ccA = a.calculateChainCodeCommitment();
let m3u = u.handleMessages(toParty([...m2u, ...m2a], 0)), m3a = a.handleMessages(toParty([...m2u, ...m2a], 1));
await report("after round2", a.toBytes());
let m4u = u.handleMessages(toParty([...m3u, ...m3a], 0), [ccU, ccA]), m4a = a.handleMessages(toParty([...m3u, ...m3a], 1), [ccU, ccA]);
await report("after round3", a.toBytes());
u.handleMessages(others([...m4u, ...m4a], 0)); a.handleMessages(others([...m4u, ...m4a], 1));
const us = u.keyshare(), as = a.keyshare();
await report("keyshare (long-lived)", as.toBytes());

console.log("\nDSG (signing) action-session blob per round:");
let su = new SignSession(us, "m"), sa = new SignSession(as, "m");
const s1u = su.createFirstMessage(), s1a = sa.createFirstMessage();
await report("after createFirst", sa.toBytes());
let s2u = su.handleMessages(others([s1u, s1a], 0)), s2a = sa.handleMessages(others([s1u, s1a], 1));
await report("after round1", sa.toBytes());
let s3u = su.handleMessages(toParty([...s2u, ...s2a], 0)), s3a = sa.handleMessages(toParty([...s2u, ...s2a], 1));
await report("after round2", sa.toBytes());
su.handleMessages(toParty([...s3u, ...s3a], 0)); sa.handleMessages(toParty([...s3u, ...s3a], 1));
await report("after round3", sa.toBytes());
