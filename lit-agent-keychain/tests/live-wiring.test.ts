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
