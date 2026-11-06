import http from "node:http";
import { writeFile } from "node:fs/promises";
import { fixture, keyFor, pubFor } from "../tests/harness.ts";
import { actionSource } from "../protocol/actions.ts";
import { hex } from "../protocol/crypto.ts";
const used = new Set<string>();
const server = http.createServer((_req, res) => {
  res.setHeader("Content-Type", "application/json");
  if (_req.url === "/api/registry/requests/consume") {
    let body = "";
    _req.on("data", (chunk) => {
      body += chunk;
    });
    _req.on("end", () => {
      const nonce = JSON.parse(body).request.nonce;
      res.statusCode = used.has(nonce) ? 409 : 200;
      used.add(nonce);
      res.end(JSON.stringify({ ok: res.statusCode === 200 }));
    });
  } else res.end(JSON.stringify({ policy: f.sign(f.policy) }));
});
await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
const f = await fixture(
  "export",
  `http://127.0.0.1:${(server.address() as any).port}`,
);
await writeFile(
  "generated/runtime-vector.json",
  JSON.stringify({
    code: actionSource(f.manifest),
    params: f.params,
    privateKey: "0x" + hex(keyFor(f.cid)),
    authorityPublicKey: "0x" + pubFor(f.authorityCid),
    responseKey: hex(f.responseKey),
    request: f.request,
    expected: f.secret,
  }),
);
await writeFile("generated/runtime-vector.pid", String(process.pid));
process.stdout.write("Runtime fixture ready\n");
