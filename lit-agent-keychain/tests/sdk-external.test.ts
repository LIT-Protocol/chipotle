import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { build } from "esbuild";

test("packed SDK works in an isolated strict TypeScript, Node and browser consumer", async (t) => {
  const dir = mkdtempSync(path.join(tmpdir(), "keychain-external-"));
  try {
    // The normal test command builds first. No monorepo dependencies or symlinks
    // are installed in the consumer; npm resolves only the tarball's dependencies.
    const packed = JSON.parse(
      execFileSync(
        "npm",
        [
          "pack",
          "./sdk",
          "--ignore-scripts",
          "--json",
          "--pack-destination",
          dir,
        ],
        { encoding: "utf8" },
      ),
    );
    writeFileSync(
      path.join(dir, "package.json"),
      JSON.stringify({ private: true, type: "module" }),
    );
    execFileSync(
      "npm",
      [
        "install",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        path.join(dir, packed[0].filename),
      ],
      { cwd: dir, stdio: "pipe" },
    );
    const consumer = `import { Keychain, ACTIONS, shapeToJsonSchema, type AgentConfig, type Shape, type ActionDefinition } from '@lit-protocol/keychain';
const config: AgentConfig = { v: 2, litApiUrl: 'http://localhost:8000', secrets: {} };
const client = new Keychain(Keychain.generateKey().privateKey, config);
const definition: ActionDefinition = ACTIONS.export;
const shape: Shape = { type: 'string', maxLength: 64 };
shapeToJsonSchema(shape);
client.destroy();
`;
    writeFileSync(path.join(dir, "consumer.mts"), consumer);
    await t.test("strict declarations resolve without skipLibCheck", () => {
      const result = spawnSync(
        process.execPath,
        [
          path.resolve("node_modules/typescript/bin/tsc"),
          "--noEmit",
          "--strict",
          "--module",
          "NodeNext",
          "--target",
          "ES2022",
          "consumer.mts",
        ],
        { cwd: dir, encoding: "utf8" },
      );
      assert.equal(result.status, 0, result.stdout + result.stderr);
    });
    await t.test("Node import automatically binds TLS", () => {
      writeFileSync(
        path.join(dir, "runtime.mjs"),
        readFileSync("tests/fixtures/sdk-consumer.mjs"),
      );
      const env: NodeJS.ProcessEnv = {
        ...process.env,
        KEYCHAIN_TEST_FIXTURES: path.resolve("tests/fixtures/attestation"),
      };
      delete env.NODE_TEST_CONTEXT;
      const result = spawnSync(process.execPath, ["runtime.mjs"], {
        cwd: dir,
        encoding: "utf8",
        env,
      });
      assert.equal(result.status, 0, result.stdout + result.stderr);
      assert.match(result.stdout, /# pass 2/);
    });
    await t.test("browser bundle is free of Node builtins", async () => {
      const result = await build({
        absWorkingDir: dir,
        entryPoints: ["consumer.mts"],
        bundle: true,
        platform: "browser",
        format: "esm",
        write: false,
        metafile: true,
      });
      assert.ok(
        Object.keys(result.metafile!.inputs).some((p) =>
          p.endsWith("dist/index.js"),
        ),
      );
      assert.ok(
        !Object.keys(result.metafile!.inputs).some((p) =>
          p.endsWith("dist/node.js"),
        ),
      );
      assert.doesNotMatch(
        result.outputFiles[0].text,
        /node:(tls|crypto)|require\(["']tls/,
      );
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
