// Test-only local server: real Worker + D1 + static dashboard, simulated Lit API
// and email delivery. No production fixture routes exist in the Worker.
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { applyMigrations } from "./migrations.mjs";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, extname } from "node:path";
process.chdir(resolve(import.meta.dirname, ".."));
const root = resolve(import.meta.dirname, "../../lit-static");
const mail = [];
const mf = new Miniflare(
  convertV4MiniflareOptions({
    workers: [
      {
        modules: true,
        scriptPath: resolve(import.meta.dirname, "../dist/worker.js"),
        compatibilityDate: "2025-10-11",
        d1Databases: ["DB"],
        bindings: {
          ENVIRONMENT: "local",
          DASHBOARD_URL: "http://localhost:8080/dapps/dashboard/",
          AUTH_SECRET: "test-only-browser-secret-123456789012345",
          RESEND_API_KEY: "test",
          MAIL_FROM: "accounts@example.com",
        },
        outboundService: async (request) => {
          mail.push(await request.json());
          return Response.json({ id: "test" });
        },
      },
    ],
  }),
);
const db = await mf.getD1Database("DB");
await applyMigrations(db);
const auth = createServer(async (req, res) => {
  try {
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    const body = Buffer.concat(chunks);
    const headers = new Headers();
    for (const [k, v] of Object.entries(req.headers))
      if (v) headers.set(k, Array.isArray(v) ? v.join(",") : v);
    const response = await mf.dispatchFetch("http://localhost:8787" + req.url, {
      method: req.method,
      headers,
      ...(body.length ? { body } : {}),
    });
    res.writeHead(response.status, Object.fromEntries(response.headers));
    res.end(Buffer.from(await response.arrayBuffer()));
  } catch {
    res.writeHead(500);
    res.end("Test server failure");
  }
});
const staticServer = createServer(async (req, res) => {
  const pathname = new URL(req.url, "http://localhost").pathname;
  if (pathname === "/__test/mail") {
    res.setHeader("Content-Type", "application/json");
    res.end(JSON.stringify(mail));
    return;
  }
  if (pathname === "/__test/records") {
    res.setHeader("Content-Type", "application/json");
    res.end(
      JSON.stringify(
        (await db.prepare("SELECT * FROM auth_users").all()).results,
      ),
    );
    return;
  }
  try {
    const path = resolve(
      root,
      "." +
        decodeURIComponent(pathname) +
        (pathname.endsWith("/") ? "index.html" : ""),
    );
    if (!path.startsWith(root + "/")) throw Error();
    const data = await readFile(path);
    res.setHeader(
      "Content-Type",
      {
        ".html": "text/html",
        ".js": "text/javascript",
        ".css": "text/css",
        ".svg": "image/svg+xml",
      }[extname(path)] || "application/octet-stream",
    );
    res.end(data);
  } catch {
    res.writeHead(404);
    res.end("Not found");
  }
});
auth.listen(8787, "localhost");
staticServer.listen(8080, "localhost");
for (const signal of ["SIGINT", "SIGTERM"])
  process.on(signal, async () => {
    auth.close();
    staticServer.close();
    await mf.dispose();
    process.exit(0);
  });
