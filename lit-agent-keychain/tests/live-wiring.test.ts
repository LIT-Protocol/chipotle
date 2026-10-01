import { test } from "node:test";
import assert from "node:assert/strict";
import { parseRunArgs, planInjection } from "../sdk/run.mjs";
import { callTool } from "../sdk/mcp.mjs";
test("run accepts identity only and async live inventory; collisions require explicit IDs", () => {
  assert.equal(
    parseRunArgs(["identity.json", "--only", "A", "--", "printenv"]).configFile,
    undefined,
  );
  assert.equal(
    parseRunArgs(["identity.json", "--", "printenv"]).identityFile,
    "identity.json",
  );
  assert.throws(
    () =>
      planInjection(
        [
          { name: "A", operation: "get" },
          { name: "A", operation: "get" },
        ],
        { only: null, rename: {} },
      ),
    /ambiguous/i,
  );
});
test("MCP lists fresh asynchronous inventory without config snapshots", async () => {
  let names = ["FIRST"];
  const client: any = {
    publicKey: "public",
    list: async () =>
      names.map((name) => ({ name, release: "export", operation: "get" })),
    get: async (name: string) => name,
  };
  assert.match(JSON.stringify(await callTool(client, "list_secrets")), /FIRST/);
  names = ["SECOND"];
  assert.match(
    JSON.stringify(await callTool(client, "list_secrets")),
    /SECOND/,
  );
  assert.doesNotMatch(
    JSON.stringify(await callTool(client, "list_secrets")),
    /FIRST/,
  );
  assert.match(
    JSON.stringify(await callTool(client, "get_secret", { name: "SECOND" })),
    /SECOND/,
  );
});
test("run injects every live secret by id; shared names need --env or --only", () => {
  const list = [
    { id: "v1/s1", name: "API_KEY", operation: "get" },
    { id: "v2/s2", name: "TOKEN", operation: "get" },
    { id: "v2/s3", name: "STRIPE", operation: "stripe.balance" },
  ];
  // Default: everything exportable, fetched by unambiguous id, exposed by name.
  assert.deepEqual(planInjection(list, { only: null, rename: {} }), {
    plan: [
      { name: "v1/s1", envVar: "API_KEY" },
      { name: "v2/s2", envVar: "TOKEN" },
    ],
    skipped: ["STRIPE"],
  });
  // Mappings may use either the unique name or the id.
  assert.deepEqual(
    planInjection(list, {
      only: null,
      rename: { "v1/s1": "KEY" },
      files: { TOKEN: "/tmp/t" },
    }).plan,
    [
      { name: "v1/s1", envVar: "KEY" },
      { name: "v2/s2", file: "/tmp/t" },
    ],
  );
  assert.deepEqual(
    planInjection(list, { only: ["v2/s2"], rename: { TOKEN: "T" } }).plan,
    [{ name: "v2/s2", envVar: "T" }],
  );
  const shared = [
    { id: "v1/s1", name: "API_KEY", operation: "get" },
    { id: "v2/s2", name: "API_KEY", operation: "get" },
  ];
  assert.throws(
    () => planInjection(shared, { only: null, rename: {} }),
    /ambiguous across vaults.*--env v1\/s1=A --env v2\/s2=B/,
  );
  assert.throws(
    () => planInjection(shared, { only: ["API_KEY"], rename: {} }),
    /ambiguous/i,
  );
  assert.throws(
    () => planInjection(shared, { only: null, rename: { API_KEY: "X" } }),
    /ambiguous/i,
  );
  assert.deepEqual(
    planInjection(shared, {
      only: null,
      rename: { "v1/s1": "A_KEY", "v2/s2": "B_KEY" },
    }).plan,
    [
      { name: "v1/s1", envVar: "A_KEY" },
      { name: "v2/s2", envVar: "B_KEY" },
    ],
  );
  assert.deepEqual(
    planInjection(shared, { only: ["v2/s2"], rename: {} }).plan,
    [{ name: "v2/s2", envVar: "API_KEY" }],
  );
});
