import { test } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { jsonFetch } from "../protocol/http.ts";
test("deadline covers the entire response body and refuses oversized or redirected responses", async () => {
  const server = http.createServer((req, res) => {
    if (req.url === "/slow") {
      res.writeHead(200, { "content-type": "application/json" });
      res.flushHeaders();
      const t = setTimeout(() => res.end('{"ok":true}'), 1000);
      res.on("close", () => clearTimeout(t));
    } else if (req.url === "/large")
      res.end(JSON.stringify({ data: "x".repeat(1000) }));
    else {
      res.writeHead(302, { location: "/large" });
      res.end();
    }
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  const origin = `http://127.0.0.1:${(server.address() as any).port}`;
  try {
    const start = Date.now();
    await assert.rejects(jsonFetch(origin + "/slow", {}, 80));
    assert.ok(Date.now() - start < 700);
    await assert.rejects(
      jsonFetch(origin + "/large", {}, 1000, 100),
      /too large/,
    );
    await assert.rejects(jsonFetch(origin + "/redirect"));
  } finally {
    server.closeAllConnections();
    await new Promise<void>((r) => server.close(() => r()));
  }
});
