import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, statSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { Keychain } from "../sdk/dist/index.js";

test("distributed SDK and CLI generate local identities without printing private keys", () => {
  const dir = mkdtempSync(path.join(tmpdir(), "keychain-cli-"));
  try {
    const file = path.join(dir, "identity.json");
    const output = execFileSync(
      process.execPath,
      ["sdk/cli.mjs", "init", file],
      { encoding: "utf8" },
    );
    const identity = JSON.parse(readFileSync(file, "utf8"));
    assert.ok(output.includes(identity.publicKey));
    assert.ok(!output.includes(identity.privateKey));
    assert.equal(statSync(file).mode & 0o777, 0o600);
    const agent = new Keychain(identity.privateKey, {
      v: 2,
      litApiUrl: "http://localhost:8000",
      secrets: {},
    });
    assert.equal(agent.publicKey, identity.publicKey);
    agent.destroy();
    assert.throws(() =>
      execFileSync(process.execPath, ["sdk/cli.mjs", "init", file], {
        stdio: "pipe",
      }),
    );
    assert.equal(
      JSON.parse(readFileSync(file, "utf8")).privateKey,
      identity.privateKey,
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
