import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import {
  Keychain,
  assertAgentIdentity,
  actionDefinition,
} from "../sdk/dist/index.js";
import { handleMessage, callTool, loadKeychain } from "../sdk/mcp.mjs";

const config = {
  v: 2 as const,
  litApiUrl: "http://localhost:8000",
  secrets: {},
};

test("identity rejects present unsupported versions and inconsistent public keys", () => {
  const identity = Keychain.generateKey();
  assert.doesNotThrow(() =>
    assertAgentIdentity({ privateKey: identity.privateKey }),
  );
  assert.doesNotThrow(() => assertAgentIdentity({ v: 2, ...identity }));
  for (const v of [999, 1, null, "2", undefined]) {
    assert.throws(
      () => assertAgentIdentity({ ...identity, v }),
      /identity version/i,
    );
  }
  for (const publicKey of ["0".repeat(64), "bad", null, undefined]) {
    assert.throws(
      () => assertAgentIdentity({ ...identity, publicKey }),
      /publicKey/i,
    );
  }
});

for (const name of ["constructor", "toString", "__proto__"]) {
  test(`inherited secret and action name ${name} is unknown`, async () => {
    const client = new Keychain(Keychain.generateKey().privateKey, config);
    try {
      await assert.rejects(client.get(name), /Unknown secret/);
      await assert.rejects(client.use(name), /Unknown secret/);
      await assert.rejects(
        callTool(client, "get_secret", { name }),
        /Unknown secret/,
      );
      assert.throws(() => actionDefinition(name), /Unknown action release/);
    } finally {
      client.destroy();
    }
  });
}

test("MCP merges own prototype-named secrets without prototype mutation", async () => {
  const identity = Keychain.generateKey();
  const locator = { manifest: { release: "export" }, actionCid: "test" };
  const secrets = Object.fromEntries(
    ["__proto__", "constructor"].map((name) => [name, locator]),
  );
  const files: Record<string, unknown> = {
    identity,
    config: { ...config, secrets },
  };
  const client = await loadKeychain("identity", ["config"], {
    readFile: async (name: string) => JSON.stringify(files[name]),
  });
  try {
    assert.deepEqual(
      client
        .list()
        .map((s) => s.name)
        .sort(),
      ["__proto__", "constructor"],
    );
    assert.equal(Object.hasOwn(client.config.secrets, "__proto__"), true);
  } finally {
    client.destroy();
  }
});

for (const id of [{}, [], true, false, null]) {
  test(`MCP rejects invalid request id ${JSON.stringify(id)}`, async () => {
    const reply = await handleMessage({} as any, {
      jsonrpc: "2.0",
      id,
      method: "ping",
    });
    assert.deepEqual(reply, {
      jsonrpc: "2.0",
      id: null,
      error: { code: -32600, message: "Invalid Request" },
    });
  });
}
test("MCP preserves valid ids and ignores notifications", async () => {
  for (const id of [0, 42, "request"]) {
    assert.deepEqual(
      await handleMessage({} as any, { jsonrpc: "2.0", id, method: "ping" }),
      { jsonrpc: "2.0", id, result: {} },
    );
  }
  assert.equal(
    await handleMessage({} as any, { jsonrpc: "2.0", method: "ping" }),
    undefined,
  );
});

test("destroyed clients fail locally before lookups or network and destroy is idempotent", async () => {
  const client = new Keychain(Keychain.generateKey().privateKey, config);
  let calls = 0;
  const fetch = globalThis.fetch;
  globalThis.fetch = async () => {
    calls++;
    throw new Error("unexpected network");
  };
  try {
    client.destroy();
    client.destroy();
    await assert.rejects(client.get("missing"), /destroyed/i);
    await assert.rejects(client.use("missing"), /destroyed/i);
    await assert.rejects(client.stripeBalance("missing"), /destroyed/i);
    await assert.rejects(async () => client.attest(), /destroyed/i);
    assert.equal(calls, 0);
  } finally {
    globalThis.fetch = fetch;
  }
});

test("CLI conventional help and version succeed on stdout", () => {
  for (const flag of ["--help", "-h", "--version", "-v"]) {
    const result = spawnSync(process.execPath, ["sdk/cli.mjs", flag], {
      encoding: "utf8",
    });
    assert.equal(result.status, 0, `${flag}: ${result.stderr}`);
    assert.equal(result.stderr, "");
    if (["--help", "-h"].includes(flag)) assert.match(result.stdout, /Usage:/);
    else
      assert.equal(
        result.stdout.trim(),
        JSON.parse(readFileSync("sdk/package.json", "utf8")).version,
      );
  }
});
