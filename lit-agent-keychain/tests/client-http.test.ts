import { test } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { jsonFetch, HttpError } from "../protocol/client-http.ts";
test("non-2xx responses surface the status and a bounded, sanitized reason", async () => {
  const server = http.createServer((req, res) => {
    if (req.url === "/json") {
      res.writeHead(403, { "content-type": "application/json" });
      res.end(
        JSON.stringify({
          message:
            "The provided API key is not authorized to execute the specified action (Qm.../0x1).",
        }),
      );
    } else if (req.url === "/nested") {
      res.writeHead(400, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: { message: "bad\u0000\nrequest" } }));
    } else if (req.url === "/html") {
      res.writeHead(502, { "content-type": "text/html" });
      res.end("<html><body>Bad gateway</body></html>");
    } else {
      res.writeHead(500);
      res.end("x".repeat(10000));
    }
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  const origin = `http://127.0.0.1:${(server.address() as any).port}`;
  try {
    await assert.rejects(jsonFetch(origin + "/json"), (e: any) => {
      assert.ok(e instanceof HttpError);
      assert.equal(e.status, 403);
      assert.match(e.message, /^Request failed \(403\): The provided API key/);
      return true;
    });
    await assert.rejects(jsonFetch(origin + "/nested"), (e: any) => {
      assert.equal(e.status, 400);
      assert.equal(e.detail, "bad request");
      return true;
    });
    await assert.rejects(jsonFetch(origin + "/html"), (e: any) => {
      assert.equal(e.status, 502);
      assert.equal(e.message, "Request failed (502)");
      return true;
    });
    await assert.rejects(jsonFetch(origin + "/long"), (e: any) => {
      assert.equal(e.status, 500);
      assert.ok(e.detail.length <= 200);
      return true;
    });
  } finally {
    server.closeAllConnections();
    await new Promise<void>((r) => server.close(() => r()));
  }
});
