import { test } from "node:test";
import assert from "node:assert/strict";
import { access, mkdtemp, readdir, rm } from "node:fs/promises";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { tmpdir } from "node:os";
import path from "node:path";
import { startPasskeyServer } from "./passkey-server.ts";

test("passkey server exposes a complete isolated build before readiness", async () => {
  const server = await startPasskeyServer(0);
  try {
    const identities = await fetch(`${server.origin}/identities.js`);
    assert.equal(identities.status, 200);
    assert.match(identities.headers.get("content-type")!, /javascript/);
    const source = await identities.text();
    assert.match(source, /createPasskey/);
    assert.match(source, /discoverPasskey/);
    assert.doesNotMatch(
      source,
      /node_modules\/\.vite|\/@vite\/client|\/src\/identities\.ts/,
    );
    const page = await fetch(server.origin);
    assert.equal(page.status, 200);
    const html = await page.text();
    const scripts = [...html.matchAll(/<script[^>]+src="([^"]+)"/g)];
    assert.ok(
      scripts.length > 0,
      "serve the built real UI, not a blank substitute",
    );
    for (const [, path] of scripts) {
      const response = await fetch(new URL(path, server.origin));
      assert.equal(response.status, 200);
      assert.match(response.headers.get("content-type")!, /javascript/);
    }
    assert.doesNotMatch(html, /\/@vite\/client|\/src\/main/);
    assert.ok(server.outDir.includes("keychain-passkey-"));
  } finally {
    await server.close();
  }
  await assert.rejects(access(server.outDir), { code: "ENOENT" });
});

test(
  "Vite SIGTERM shutdown removes the isolated build",
  { timeout: 90000 },
  async () => {
    const temp = await mkdtemp(path.join(tmpdir(), "passkey-shutdown-test-"));
    const child = spawn(
      process.execPath,
      [
        "--import",
        "tsx",
        "--input-type=module",
        "-e",
        'import { startPasskeyServer } from "./passkey-server.ts"; await startPasskeyServer(0); console.log("READY");',
      ],
      {
        cwd: import.meta.dirname,
        env: { ...process.env, TMPDIR: temp, CI: "true" },
        stdio: ["ignore", "pipe", "pipe"],
      },
    );
    let errors = "";
    child.stderr.on("data", (data) => {
      errors += data;
    });
    const exited = once(child, "exit");
    try {
      await Promise.race([
        once(child.stdout, "data").then(([data]) =>
          assert.match(String(data), /READY/),
        ),
        exited.then(() => {
          throw new Error(`Server exited before readiness: ${errors}`);
        }),
      ]);
      assert.ok(
        (await readdir(temp)).some((name) =>
          name.startsWith("keychain-passkey-"),
        ),
      );
      child.kill("SIGTERM");
      const [code] = await exited;
      assert.ok(code === 0 || code === 143, `SIGTERM exit ${code}: ${errors}`);
      assert.deepEqual(
        (await readdir(temp)).filter((name) =>
          name.startsWith("keychain-passkey-"),
        ),
        [],
      );
    } finally {
      if (child.exitCode === null && child.signalCode === null) {
        child.kill("SIGKILL");
        await exited;
      }
      await rm(temp, { recursive: true, force: true });
    }
  },
);
