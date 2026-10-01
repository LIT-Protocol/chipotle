import { writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
const required = (name) => {
  const value = process.env[name];
  if (!value) throw new Error(`Missing ${name}`);
  return value;
};
const environment = required("AUTH_ENVIRONMENT");
if (!["staging", "production"].includes(environment))
  throw new Error("Use staging or production.");
const databaseId = required("AUTH_DATABASE_ID");
if (!/^[0-9a-f-]{36}$/.test(databaseId))
  throw new Error("Invalid D1 database ID.");
const dashboard = new URL(required("AUTH_DASHBOARD_URL")),
  domain = required("AUTH_DOMAIN");
if (dashboard.protocol !== "https:" || !/^[a-z0-9.-]+$/.test(domain))
  throw new Error("Use HTTPS and a valid auth domain.");
const secrets = {
  AUTH_SECRET: required("AUTH_SECRET"),
  RESEND_API_KEY: required("RESEND_API_KEY"),
};
if (secrets.AUTH_SECRET.length < 32)
  throw new Error(
    "AUTH_SECRET must contain at least 32 characters of randomness.",
  );
const config = {
  name: `chipotle-dashboard-auth-${environment}`,
  main: "src/index.ts",
  compatibility_date: "2025-10-11",
  workers_dev: false,
  vars: {
    ENVIRONMENT: environment,
    DASHBOARD_URL: dashboard.href,
    MAIL_FROM: required("AUTH_MAIL_FROM"),
  },
  routes: [{ pattern: domain, custom_domain: true }],
  d1_databases: [
    {
      binding: "DB",
      database_name: `chipotle-auth-${environment}`,
      database_id: databaseId,
      migrations_dir: "migrations",
    },
  ],
  triggers: { crons: ["*/5 * * * *"] },
  observability: { enabled: false },
};
await writeFile("wrangler.deploy.json", JSON.stringify(config, null, 2) + "\n");
function run(args, input) {
  const result = spawnSync(
    "pnpm",
    ["exec", "wrangler", ...args, "--config", "wrangler.deploy.json"],
    { stdio: input ? ["pipe", "inherit", "inherit"] : "inherit", input },
  );
  if (result.status !== 0) process.exit(result.status || 1);
}
run(["d1", "migrations", "apply", "DB", "--remote"]);
run(["deploy"]);
run(["secret", "bulk"], JSON.stringify(secrets));
