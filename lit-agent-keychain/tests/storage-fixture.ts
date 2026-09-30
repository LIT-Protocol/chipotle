import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
export function fixtureSql(vault: string, sql: string) {
  const database = new URL(process.env.KEYCHAIN_TEST_DATABASE_URL!);
  assert.ok(["127.0.0.1", "localhost"].includes(database.hostname));
  assert.match(database.pathname, /test|_ci$/);
  assert.match(vault, /^[0-9a-f]{64}$/);
  execFileSync(
    "psql",
    [
      database.toString(),
      "-X",
      "-v",
      "ON_ERROR_STOP=1",
      "-v",
      `vault=${vault}`,
    ],
    { input: sql },
  );
}
