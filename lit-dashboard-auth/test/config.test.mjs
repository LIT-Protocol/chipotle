import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";
test("static configuration accepts empty/HTTPS origins and rejects URLs with credentials or code", async () => {
  const directory = await mkdtemp(join(tmpdir(), "chipotle-static-"));
  const folder = join(directory, "lit-static/dapps/dashboard");
  await mkdir(folder, { recursive: true });
  const target = join(folder, "password-client.js");
  const original = await readFile(
    "../lit-static/dapps/dashboard/password-client.js",
    "utf8",
  );
  try {
    for (const url of ["", "https://auth.example.com/"]) {
      await writeFile(target, original);
      execFileSync("python3", [resolve("scripts/configure-static.py")], {
        cwd: directory,
        env: { ...process.env, AUTH_URL: url },
      });
      const updated = await readFile(target, "utf8");
      assert.ok(!updated.includes("__LIT_AUTH_BASE_URL__"));
      assert.ok(updated.includes(JSON.stringify(url.replace(/\/$/, ""))));
    }
    for (const url of [
      "http://auth.example.com",
      "https://user:pass@auth.example.com",
      "https://auth.example.com/path",
      "https://auth.example.com?x=1",
    ]) {
      await writeFile(target, original);
      assert.throws(() =>
        execFileSync("python3", [resolve("scripts/configure-static.py")], {
          cwd: directory,
          env: { ...process.env, AUTH_URL: url },
          stdio: "pipe",
        }),
      );
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
