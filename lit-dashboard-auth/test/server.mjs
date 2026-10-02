// Test-only launcher for the dashboard auth service: the real lit-payments
// binary against a fresh local Postgres database, with outbound Resend calls
// captured by a local fake. Nothing here exists in production.
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { randomBytes } from "node:crypto";
import { access, constants } from "node:fs/promises";
import { resolve } from "node:path";
import pg from "pg";

const root = resolve(import.meta.dirname, "../..");
export const AUTH_PORT = 8787;
export const MAIL_PORT = 8790;
export const AUTH_SECRET = "test-only-browser-secret-123456789012345";

function parseAdminUrl() {
  // Local developers: Postgres.app / Homebrew with the current user. CI: the
  // postgres service container.
  return (
    process.env.TEST_DATABASE_URL ||
    process.env.DATABASE_URL ||
    "postgres://localhost:5432/postgres"
  );
}

async function waitFor(check, child, timeoutMs = 60000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (child.exitCode !== null)
      throw new Error(`lit-payments exited with code ${child.exitCode} during startup`);
    try {
      if (await check()) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error("lit-payments did not become ready in time");
}

/**
 * Start the stack. `dashboardUrls` are the dashboard origins the service will
 * accept (the first is used by the API tests). Returns handles for captured
 * mail, a pg client on the fresh database, and `stop()`.
 */
export async function startAuthService({ dashboardUrls, logStream = process.stderr } = {}) {
  const binary =
    process.env.LIT_PAYMENTS_BIN || resolve(root, "lit-payments/target/debug/lit-payments");
  try {
    await access(binary, constants.X_OK);
  } catch {
    throw new Error(
      `Missing ${binary}. Build it with: cargo build --manifest-path lit-payments/Cargo.toml (or set LIT_PAYMENTS_BIN)`,
    );
  }

  const adminUrl = parseAdminUrl();
  const database = `chipotle_auth_test_${randomBytes(6).toString("hex")}`;
  const admin = new pg.Client({ connectionString: adminUrl });
  await admin.connect();
  await admin.query(`CREATE DATABASE ${database}`);
  const databaseUrl = new URL(adminUrl);
  databaseUrl.pathname = `/${database}`;

  const mail = [];
  const fakeResend = createServer(async (req, res) => {
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    if (req.method === "POST" && req.url === "/emails") {
      try {
        const message = JSON.parse(Buffer.concat(chunks).toString());
        mail.push({ ...message, idempotencyKey: req.headers["idempotency-key"] });
      } catch {}
      res.setHeader("Content-Type", "application/json");
      res.end(JSON.stringify({ id: `test-${mail.length}` }));
      return;
    }
    res.writeHead(404);
    res.end();
  });
  await new Promise((r) => fakeResend.listen(MAIL_PORT, "127.0.0.1", r));

  const env = {
    ...process.env,
    RUST_LOG: process.env.LIT_PAYMENTS_LOG || "warn,lit_payments=info",
    ROCKET_ADDRESS: "127.0.0.1",
    ROCKET_PORT: String(AUTH_PORT),
    ROCKET_SECRET_KEY: Buffer.alloc(32, 1).toString("base64"),
    DATABASE_URL: databaseUrl.toString(),
    PUBLIC_BASE_URL: `http://localhost:${AUTH_PORT}`,
    MAGIC_LINK_SIGNING_KEY: Buffer.alloc(32, 2).toString("base64"),
    RESEND_API_KEY: "test",
    RESEND_API_BASE_URL: `http://127.0.0.1:${MAIL_PORT}`,
    MAIL_FROM: "accounts@example.com",
    // Unrelated lit-payments features need values to boot; none are reachable.
    STRIPE_SECRET_KEY: "sk_test_disabled",
    STRIPE_PUBLISHABLE_KEY: "pk_test_disabled",
    STRIPE_WEBHOOK_SECRET: "whsec_disabled",
    LIT_API_SERVER_BASE_URL: "http://127.0.0.1:9",
    LIT_INTERNAL_SHARED_SECRET: "test-only-internal-secret",
    LIT_ACCOUNTS_RPC_URL: "http://127.0.0.1:9",
    LIT_ACCOUNTS_CHAIN_ID: "31337",
    LIT_ACCOUNTS_CONTRACT_ADDRESS: "0x" + "00".repeat(20),
    RECONCILER_INTERVAL_SECS: "86400",
    ENTERPRISE_BILLING_INTERVAL_SECS: "86400",
    // Dashboard auth under test.
    DASHBOARD_AUTH_SECRET: AUTH_SECRET,
    DASHBOARD_AUTH_URLS: dashboardUrls.join(","),
    DASHBOARD_AUTH_ENVIRONMENT: "local",
    DASHBOARD_AUTH_COOKIE_SAMESITE: "lax",
    DASHBOARD_AUTH_OUTBOX_INTERVAL_SECS: "5",
  };
  const child = spawn(binary, [], {
    cwd: resolve(root, "lit-payments"),
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.pipe(logStream, { end: false });
  child.stderr.pipe(logStream, { end: false });
  await waitFor(
    async () => (await fetch(`http://localhost:${AUTH_PORT}/health`)).ok,
    child,
  );

  const db = new pg.Client({ connectionString: databaseUrl.toString() });
  await db.connect();

  let stopped = false;
  async function stop() {
    if (stopped) return;
    stopped = true;
    await db.end().catch(() => {});
    if (child.exitCode === null) {
      child.kill("SIGTERM");
      await new Promise((r) => {
        const timer = setTimeout(() => {
          child.kill("SIGKILL");
          r();
        }, 5000);
        child.once("exit", () => {
          clearTimeout(timer);
          r();
        });
      });
    }
    fakeResend.close();
    await admin.query(`DROP DATABASE IF EXISTS ${database} WITH (FORCE)`).catch(() => {});
    await admin.end().catch(() => {});
  }
  return {
    mail,
    db,
    child,
    url: `http://localhost:${AUTH_PORT}`,
    records: async () => (await db.query("SELECT * FROM dashboard_auth_users")).rows,
    stop,
  };
}
