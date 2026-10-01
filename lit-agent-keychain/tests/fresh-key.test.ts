import { test } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { LitConnection, HttpError } from "../sdk/src/index.ts";
const KEY = "02" + "ab".repeat(32);
const CID = "Qm" + "a".repeat(44);
function litStub(denials: number, status = 403) {
  let calls = 0;
  const server = http.createServer(async (req, res) => {
    for await (const _ of req) void _;
    calls++;
    if (calls <= denials) {
      res.writeHead(status, { "content-type": "application/json" });
      res.end(
        JSON.stringify({
          message:
            "The provided API key is not authorized to execute the specified action",
        }),
      );
      return;
    }
    res.writeHead(200, { "content-type": "application/json" });
    res.end(
      JSON.stringify({
        has_error: false,
        response: { ok: true, public_key: KEY },
      }),
    );
  });
  return {
    server,
    calls: () => calls,
    async start() {
      await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
      return `http://127.0.0.1:${(server.address() as any).port}`;
    },
    async stop() {
      server.closeAllConnections();
      await new Promise<void>((r) => server.close(() => r()));
    },
  };
}
test("a freshly minted usage key retries through transient 403s from Chipotle", async () => {
  const stub = litStub(2);
  const url = await stub.start();
  try {
    const lit = new LitConnection(url, 5000, "fresh-key", false);
    const waits: number[] = [];
    const key = await lit.publicKey(CID, {
      ms: 5000,
      stepMs: 10,
      onWait: (n) => waits.push(n),
    });
    assert.equal(key, KEY);
    assert.equal(stub.calls(), 3);
    assert.deepEqual(waits, [1, 2]);
    // Cached after success; no further network calls.
    assert.equal(await lit.publicKey(CID), KEY);
    assert.equal(stub.calls(), 3);
  } finally {
    await stub.stop();
  }
});
test("the retry is bounded and a persistent denial still fails with Chipotle's reason", async () => {
  const stub = litStub(Infinity);
  const url = await stub.start();
  try {
    const lit = new LitConnection(url, 5000, "fresh-key", false);
    const start = Date.now();
    await assert.rejects(
      lit.publicKey(CID, { ms: 120, stepMs: 10 }),
      (e: any) => {
        assert.ok(e instanceof HttpError);
        assert.equal(e.status, 403);
        assert.match(e.message, /not authorized to execute/);
        return true;
      },
    );
    assert.ok(Date.now() - start < 2000);
    assert.ok(stub.calls() >= 2);
  } finally {
    await stub.stop();
  }
});
test("without a settle policy a denial is not retried", async () => {
  const stub = litStub(1);
  const url = await stub.start();
  try {
    const lit = new LitConnection(url, 5000, "fresh-key", false);
    await assert.rejects(lit.publicKey(CID), /Request failed \(403\)/);
    assert.equal(stub.calls(), 1);
  } finally {
    await stub.stop();
  }
});
