// Test-only local server: real lit-payments auth service + Postgres + static
// dashboard, with simulated Lit API (mocked by the browser tests) and captured
// email delivery. No production fixture routes exist in the service.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, extname } from "node:path";
import { startAuthService } from "./server.mjs";

const root = resolve(import.meta.dirname, "../../lit-static");
// The real-backend CI stack uses 8088; isolated browser tests use 8080.
const staticPort = Number(process.env.DASHBOARD_TEST_PORT || 8080);
if (![8080, 8088].includes(staticPort)) throw new Error("Invalid test port");

const service = await startAuthService({
  dashboardUrls: [`http://localhost:${staticPort}/dapps/dashboard/`],
});

const staticServer = createServer(async (req, res) => {
  const pathname = new URL(req.url, "http://localhost").pathname;
  if (pathname === "/__test/mail") {
    res.setHeader("Content-Type", "application/json");
    res.end(JSON.stringify(service.mail));
    return;
  }
  if (pathname === "/__test/records") {
    res.setHeader("Content-Type", "application/json");
    res.end(JSON.stringify(await service.records()));
    return;
  }
  try {
    const path = resolve(
      root,
      "." + decodeURIComponent(pathname) + (pathname.endsWith("/") ? "index.html" : ""),
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
staticServer.listen(staticPort, "localhost");

async function shutdown() {
  staticServer.close();
  await service.stop();
  process.exit(0);
}
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, shutdown);
service.child.once("exit", (code) => {
  console.error(`lit-payments exited (${code})`);
  staticServer.close();
  process.exit(1);
});
