import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  mkdtempSync,
  readFileSync,
  statSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  Keychain,
  assertAgentConfig,
  assertAgentIdentity,
  describeCredential,
} from "../sdk/dist/index.js";

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

test("SDK rejects swapped or mistaken credentials with self-describing errors", () => {
  const identity = Keychain.generateKey();
  const usageApiKey = Buffer.alloc(32, 7).toString("base64");
  const config = {
    v: 2 as const,
    litApiUrl: "http://localhost:8000",
    usageApiKey,
    secrets: {},
  };
  assert.equal(describeCredential({ v: 2, ...identity }), "agent-identity");
  assert.equal(describeCredential(config), "agent-config");
  assert.equal(describeCredential(identity.privateKey), "agent-private-key");
  assert.equal(describeCredential(usageApiKey), "usage-api-key");
  assert.equal(describeCredential("sk_live_not_a_key"), "unknown");
  // Identity and config swapped.
  assert.throws(
    () => assertAgentIdentity(config),
    /agent config .*not an agent identity/,
  );
  assert.throws(
    () => assertAgentConfig({ v: 2, ...identity }),
    /agent identity, not an agent config/,
  );
  // A secret value or config object passed where a private key belongs.
  assert.throws(
    () => new Keychain("sk_live_not_a_key", config),
    /privateKey must be 64 lowercase hex/,
  );
  // The identity private key handed over as the billing key.
  assert.throws(
    () =>
      new Keychain(identity.privateKey, config, {
        usageApiKey: identity.privateKey,
      }),
    /looks like an agent private key/,
  );
  assert.throws(
    () =>
      new Keychain(identity.privateKey, {
        ...config,
        usageApiKey: JSON.stringify(config),
      }),
    /looks like a JSON file/,
  );
  assert.throws(
    () => new Keychain(identity.privateKey, { ...config, v: 1 } as any),
    /Unsupported agent config version/,
  );
  const agent = new Keychain(identity.privateKey, config);
  assert.deepEqual(agent.list(), []);
  agent.destroy();
});

test("stdio MCP server speaks JSON-RPC, exposes tools, and never prints the private key", async () => {
  const dir = mkdtempSync(path.join(tmpdir(), "keychain-mcp-"));
  try {
    const identityFile = path.join(dir, "identity.json");
    execFileSync(process.execPath, ["sdk/cli.mjs", "init", identityFile]);
    const identity = JSON.parse(readFileSync(identityFile, "utf8"));
    const manifest = {
      v: 2,
      network: "test",
      registry: "http://localhost:8001",
      vaultId: "0".repeat(64),
      authorityCid: "bafkreib" + "a".repeat(51),
      secretId: "1".repeat(64),
      release: "export",
    };
    const stripe = {
      ...manifest,
      secretId: "2".repeat(64),
      release: "stripe_balance",
    };
    const cid = "bafkreic" + "b".repeat(51);
    const write = (name: string, secrets: object) =>
      writeFileSync(
        path.join(dir, name),
        JSON.stringify({
          v: 2,
          litApiUrl: "http://localhost:8000",
          usageApiKey: Buffer.alloc(32, 9).toString("base64"),
          secrets,
        }),
      );
    write("A.keychain.json", { A: { manifest, actionCid: cid } });
    write("B.keychain.json", { B: { manifest: stripe, actionCid: cid } });
    const frames = [
      {
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
          protocolVersion: "2025-03-26",
          capabilities: {},
          clientInfo: { name: "test", version: "0" },
        },
      },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/list" },
      {
        jsonrpc: "2.0",
        id: 3,
        method: "tools/call",
        params: { name: "list_secrets", arguments: {} },
      },
      {
        jsonrpc: "2.0",
        id: 4,
        method: "tools/call",
        params: { name: "agent_public_key" },
      },
      {
        jsonrpc: "2.0",
        id: 5,
        method: "tools/call",
        params: { name: "get_secret", arguments: { name: "MISSING" } },
      },
      { jsonrpc: "2.0", id: 6, method: "resources/list" },
      { jsonrpc: "2.0", id: 7, method: "ping" },
    ];
    const stdout = execFileSync(
      process.execPath,
      [
        "sdk/cli.mjs",
        "mcp",
        identityFile,
        path.join(dir, "A.keychain.json"),
        path.join(dir, "B.keychain.json"),
      ],
      {
        input: frames.map((f) => JSON.stringify(f)).join("\n") + "\n",
        encoding: "utf8",
        stdio: ["pipe", "pipe", "pipe"],
      },
    );
    assert.ok(!stdout.includes(identity.privateKey));
    const replies = stdout
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line));
    assert.deepEqual(
      replies.map((r) => r.id),
      [1, 2, 3, 4, 5, 6, 7],
    );
    assert.equal(replies[0].result.protocolVersion, "2025-03-26");
    assert.deepEqual(replies[0].result.capabilities, { tools: {} });
    assert.deepEqual(
      replies[1].result.tools.map((t: any) => t.name),
      [
        "list_secrets",
        "get_secret",
        "github_read_file",
        "openai_chat",
        "slack_post_message",
        "stripe_balance",
        "supabase_tables",
        "list_actions",
        "agent_public_key",
      ],
    );
    assert.deepEqual(JSON.parse(replies[2].result.content[0].text), {
      secrets: [
        { name: "A", release: "export", operation: "get" },
        {
          name: "B",
          release: "stripe_balance",
          operation: "stripe.balance",
          input: null,
        },
      ],
    });
    assert.deepEqual(JSON.parse(replies[3].result.content[0].text), {
      publicKey: identity.publicKey,
    });
    assert.equal(replies[4].result.isError, true);
    assert.match(replies[4].result.content[0].text, /Unknown secret "MISSING"/);
    assert.equal(replies[5].error.code, -32601);
    assert.deepEqual(replies[6].result, {});
    // Swapped arguments fail before serving anything.
    assert.throws(
      () =>
        execFileSync(
          process.execPath,
          [
            "sdk/cli.mjs",
            "mcp",
            path.join(dir, "A.keychain.json"),
            identityFile,
          ],
          { input: "", stdio: "pipe" },
        ),
      /not an agent identity/,
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("`keychain run` parses its arguments and refuses anything it cannot inject", async () => {
  const { parseRunArgs, planInjection, runWithSecrets } =
    await import("../sdk/run.mjs");
  assert.deepEqual(
    parseRunArgs([
      "id.json",
      "cfg.json",
      "--only",
      "A, B",
      "--env",
      "A=STRIPE_KEY",
      "--",
      "stripe",
      "balance",
      "--live",
    ]),
    {
      identityFile: "id.json",
      configFile: "cfg.json",
      only: ["A", "B"],
      rename: { A: "STRIPE_KEY" },
      command: ["stripe", "balance", "--live"],
    },
  );
  assert.throws(() => parseRunArgs(["id", "cfg", "echo"]), /`--`/);
  assert.throws(() => parseRunArgs(["id", "cfg", "--"]), /command after/);
  assert.throws(() => parseRunArgs(["id", "--", "echo"]), /identity-file/);
  assert.throws(
    () => parseRunArgs(["id", "cfg", "--env", "A=1BAD", "--", "x"]),
    /not a valid environment variable/,
  );
  assert.throws(
    () => parseRunArgs(["id", "cfg", "--verbose", "--", "x"]),
    /Unknown run option/,
  );
  assert.throws(
    () => parseRunArgs(["id", "cfg", "--only", "--", "x"]),
    /needs a value/,
  );

  const list = [
    { name: "API_KEY", release: "export", operation: "get" },
    { name: "my-token", release: "export", operation: "get" },
    { name: "STRIPE", release: "stripe_balance", operation: "stripe.balance" },
  ];
  assert.deepEqual(
    planInjection(list, { only: null, rename: { "my-token": "MY_TOKEN" } }),
    {
      plan: [
        { name: "API_KEY", envVar: "API_KEY" },
        { name: "my-token", envVar: "MY_TOKEN" },
      ],
      skipped: ["STRIPE"],
    },
  );
  assert.throws(
    () => planInjection(list, { only: null, rename: {} }),
    /"my-token" is not a valid environment variable name/,
  );
  assert.throws(
    () => planInjection(list, { only: ["STRIPE"], rename: {} }),
    /use inside Lit/,
  );
  assert.throws(
    () => planInjection(list, { only: ["NOPE"], rename: {} }),
    /Unknown secret "NOPE"/,
  );
  assert.throws(
    () => planInjection(list, { only: null, rename: { NOPE: "X" } }),
    /Unknown secret "NOPE"/,
  );
  assert.throws(
    () =>
      planInjection(list, {
        only: ["API_KEY", "my-token"],
        rename: { "my-token": "API_KEY" },
      }),
    /both map to API_KEY/,
  );
  assert.throws(
    () => planInjection([list[2]], { only: null, rename: {} }),
    /no export-release secrets/,
  );

  // The spawn contract: secrets land only in the child's env, the parent's
  // env is untouched, the client is destroyed, and the child's exit code wins.
  let destroyed = false;
  const client = {
    list: () => list,
    get: async (name: string) => `value-of-${name}`,
    destroy: () => {
      destroyed = true;
    },
  };
  const parentEnv = { PATH: "/bin" };
  const stderr: string[] = [];
  const spawned: any[] = [];
  const spawn = (file: string, args: string[], options: any) => {
    spawned.push({ file, args, env: { ...options.env }, stdio: options.stdio });
    const handlers: Record<string, Function> = {};
    return {
      once(event: string, handler: Function) {
        handlers[event] = handler;
        if (event === "exit") setImmediate(() => handler(3, null));
      },
      kill() {},
    };
  };
  const fakeProcess = { on() {}, off() {} };
  const code = await runWithSecrets(
    client,
    { only: null, rename: { "my-token": "MY_TOKEN" }, command: ["env"] },
    {
      spawn,
      env: parentEnv,
      stderr: { write: (s: string) => stderr.push(s) },
      process: fakeProcess,
    },
  );
  assert.equal(code, 3);
  assert.equal(destroyed, true);
  assert.deepEqual(parentEnv, { PATH: "/bin" });
  assert.deepEqual(spawned, [
    {
      file: "env",
      args: [],
      env: {
        PATH: "/bin",
        API_KEY: "value-of-API_KEY",
        MY_TOKEN: "value-of-my-token",
      },
      stdio: "inherit",
    },
  ]);
  assert.match(stderr.join(""), /skipping "STRIPE"/);
  assert.ok(!stderr.join("").includes("value-of"));

  // A missing executable is reported without leaking anything.
  await assert.rejects(
    runWithSecrets(
      client,
      { only: ["API_KEY"], rename: {}, command: ["/nonexistent/bin"] },
      {
        spawn: () => ({
          once(event: string, handler: Function) {
            if (event === "error")
              setImmediate(() => handler(new Error("ENOENT")));
          },
          kill() {},
        }),
        env: {},
        stderr: { write() {} },
        process: fakeProcess,
      },
    ),
    /Cannot start \/nonexistent\/bin: ENOENT/,
  );
});

test("`keychain run` via the CLI rejects bad arguments before touching the network", () => {
  const dir = mkdtempSync(path.join(tmpdir(), "keychain-run-"));
  try {
    const identityFile = path.join(dir, "identity.json");
    execFileSync(process.execPath, ["sdk/cli.mjs", "init", identityFile]);
    const configFile = path.join(dir, "A.keychain.json");
    writeFileSync(
      configFile,
      JSON.stringify({
        v: 2,
        litApiUrl: "http://localhost:8000",
        usageApiKey: Buffer.alloc(32, 9).toString("base64"),
        secrets: {
          A: {
            manifest: {
              v: 2,
              network: "test",
              registry: "http://localhost:8001",
              vaultId: "0".repeat(64),
              authorityCid: "bafkreib" + "a".repeat(51),
              secretId: "1".repeat(64),
              release: "stripe_balance",
            },
            actionCid: "bafkreic" + "b".repeat(51),
          },
        },
      }),
    );
    const attempt = (args: string[]) => {
      try {
        execFileSync(process.execPath, ["sdk/cli.mjs", "run", ...args], {
          stdio: "pipe",
          env: { ...process.env, KEYCHAIN_SKIP_ATTESTATION: "1" },
        });
        return null;
      } catch (error: any) {
        return { status: error.status, stderr: String(error.stderr) };
      }
    };
    assert.match(attempt([identityFile, configFile, "echo"])!.stderr, /`--`/);
    assert.match(
      attempt([configFile, identityFile, "--", "echo"])!.stderr,
      /not an agent identity/,
    );
    const useOnly = attempt([identityFile, configFile, "--", "echo"])!;
    assert.equal(useOnly.status, 1);
    assert.match(useOnly.stderr, /no export-release secrets/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
