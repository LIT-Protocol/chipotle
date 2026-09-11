#!/usr/bin/env node
import { readFile, writeFile } from "node:fs/promises";
import {
  Keychain,
  assertAgentConfig,
  assertAgentIdentity,
} from "./dist/index.js";
const [command, ...args] = process.argv.slice(2);
const usage =
  "Usage:\n" +
  "  keychain init <identity-file>\n" +
  "  keychain get <identity-file> <config-file> <secret-name>\n" +
  "  keychain stripe-balance <identity-file> <config-file> <secret-name>\n" +
  "  keychain mcp <identity-file> <config-file> [more-config-files]\n" +
  "\n" +
  "identity-file: JSON from `keychain init` ({ v, privateKey, publicKey }); keep private.\n" +
  "config-file:   *.keychain.json downloaded from Keychain ({ v, litApiUrl, usageApiKey, secrets }).\n" +
  "CHIPOTLE_USAGE_API_KEY overrides the config's scoped billing key.\n";
const readJson = async (file) => {
  try {
    return JSON.parse(await readFile(file, "utf8"));
  } catch (error) {
    throw new Error(`Cannot read ${file}: ${error.message}`);
  }
};
try {
  if (command === "init" && args.length === 1) {
    const identity = Keychain.generateKey();
    await writeFile(
      args[0],
      JSON.stringify({ v: 2, ...identity }, null, 2) + "\n",
      { mode: 0o600, flag: "wx" },
    );
    process.stdout.write(
      `Agent public key: ${identity.publicKey}\nPrivate identity saved to ${args[0]}\n`,
    );
  } else if (["get", "stripe-balance"].includes(command) && args.length === 3) {
    const [identityFile, configFile, name] = args;
    const identity = await readJson(identityFile);
    assertAgentIdentity(identity);
    const config = await readJson(configFile);
    assertAgentConfig(config);
    const client = new Keychain(identity.privateKey, config, {
      usageApiKey: process.env.CHIPOTLE_USAGE_API_KEY,
    });
    try {
      const result =
        command === "get"
          ? await client.get(name)
          : await client.stripeBalance(name);
      process.stdout.write(
        (typeof result === "string" ? result : JSON.stringify(result)) + "\n",
      );
    } finally {
      client.destroy();
    }
  } else if (command === "mcp" && args.length >= 2) {
    const { main } = await import("./mcp.mjs");
    await main(args, {
      readFile,
      stdin: process.stdin,
      stdout: process.stdout,
      stderr: process.stderr,
      env: process.env,
    });
  } else {
    process.stderr.write(usage);
    process.exitCode = 2;
  }
} catch (error) {
  process.stderr.write(`Keychain: ${error.message}\n`);
  process.exitCode = 1;
}
