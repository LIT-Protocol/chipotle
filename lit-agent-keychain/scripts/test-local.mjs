// Requires a dedicated local test database. Never targets production Lit/Google.
import { spawn } from "node:child_process";
import net from "node:net";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const root = path.resolve(import.meta.dirname, "..");
const database = process.env.KEYCHAIN_TEST_DATABASE_URL;
if (!database)
  throw new Error(
    "Set KEYCHAIN_TEST_DATABASE_URL to a dedicated local test database",
  );
const db = new URL(database);
if (
  !["localhost", "127.0.0.1"].includes(db.hostname) ||
  !/test|_ci$/.test(db.pathname)
) {
  throw new Error("Only a loopback database named test or *_ci is accepted");
}
const api = "http://localhost:55441";
const lit = "http://127.0.0.1:55440";
const env = {
  ...process.env,
  DATABASE_URL: database,
  KEYCHAIN_TEST_DATABASE_URL: database,
  KEYCHAIN_TEST_API: api,
  KEYCHAIN_TEST_LIT: lit,
  PUBLIC_BASE_URL: api,
  LIT_API_URL: lit,
  VITE_LIT_API_URL: lit,
  LIT_EXECUTION_KEY: "local-test-only",
  GOOGLE_CLIENT_ID: "test.apps.googleusercontent.com",
  LIT_NETWORK: "test",
  ROCKET_PORT: "55441",
  ROCKET_ADDRESS: "127.0.0.1",
  WEB_DIR: "web/dist",
  MAX_SECRETS_PER_VAULT: "3",
  HOURLY_IP_EXECUTION_LIMIT: "10000",
};
function start(command, args) {
  const child = spawn(command, args, { cwd: root, env, stdio: "inherit" });
  child.on("error", () => {});
  return child;
}
async function run(command, args) {
  const child = start(command, args);
  await new Promise((resolve, reject) => {
    child.on("error", reject);
    child.on("exit", (code) =>
      code === 0 ? resolve() : reject(new Error(`${command} exited ${code}`)),
    );
  });
}
for (const port of [55440, 55441]) {
  await new Promise((resolve, reject) => {
    const probe = net.createServer();
    probe.once("error", reject);
    probe.listen(port, "127.0.0.1", () => probe.close(resolve));
  });
}
await run("npm", ["run", "build"]);
await run("cargo", ["+1.91", "build", "--locked"]);
const children = [
  start(process.execPath, ["--import", "tsx", "tests/mock-lit.ts"]),
  start(path.join(root, "target/debug/lit-agent-keychain"), []),
];
const stop = () => children.forEach((child) => child.kill("SIGTERM"));
process.once("SIGINT", stop);
process.once("SIGTERM", stop);
try {
  for (const url of [api + "/health", lit + "/test/google-token?nonce=ready"]) {
    const deadline = Date.now() + 30000;
    for (;;) {
      if (children.some((child) => child.exitCode !== null))
        throw new Error("Test service stopped");
      try {
        if ((await fetch(url, { signal: AbortSignal.timeout(1000) })).ok) break;
      } catch {}
      if (Date.now() > deadline)
        throw new Error(`Test service did not start: ${url}`);
      await delay(100);
    }
  }
  await run(process.execPath, [
    "--import",
    "tsx",
    "--test",
    "tests/api.test.ts",
  ]);
  await run("cargo", ["+1.91", "test", "--locked"]);
  await run("npm", ["run", "test:browser"]);
} finally {
  stop();
}
