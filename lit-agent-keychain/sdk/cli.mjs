#!/usr/bin/env node
import { readFile, writeFile } from "node:fs/promises";
import { Keychain } from "./dist/index.js";
const [command, ...args] = process.argv.slice(2);
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
    const identity = JSON.parse(await readFile(identityFile, "utf8"));
    const config = JSON.parse(await readFile(configFile, "utf8"));
    const client = new Keychain(identity.privateKey, config);
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
  } else {
    process.stderr.write(
      "Usage:\n  keychain init <identity-file>\n  keychain get <identity-file> <config-file> <secret-name>\n  keychain stripe-balance <identity-file> <config-file> <secret-name>\n",
    );
    process.exitCode = 2;
  }
} catch (error) {
  process.stderr.write(`Keychain: ${error.message}\n`);
  process.exitCode = 1;
}
