#!/usr/bin/env node
import { readFile, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import {
  Keychain,
  ACTIONS,
  assertAgentConfig,
  assertAgentIdentity,
} from "./dist/index.js";
import { peerCertificateSha256 } from "./tls.mjs";
const [command, ...args] = process.argv.slice(2);
const usage =
  "Usage:\n" +
  "  keychain init <identity-file>\n" +
  "  keychain get <identity-file> <config-file> <secret-name>\n" +
  "  keychain use <identity-file> <config-file> <secret-name> [json-input]\n" +
  "  keychain run <identity-file> <config-file> [--only A,B] [--env SECRET=ENV_VAR]... [--file SECRET=PATH]... -- <command> [args...]\n" +
  "  keychain actions\n" +
  "  keychain mcp <identity-file> <config-file> [more-config-files]\n" +
  "  keychain attest [lit-api-url]\n" +
  "\n" +
  "identity-file: JSON from `keychain init` ({ v, privateKey, publicKey }); keep private.\n" +
  "config-file:   *.keychain.json downloaded from Keychain ({ v, litApiUrl, usageApiKey, secrets }).\n" +
  "use runs the secret's catalog action inside Lit (never revealing the value); actions lists the catalog.\n" +
  "run decrypts export-release secrets into the command's environment (named after each secret) and\n" +
  "  exits with its status; nothing is printed. --only picks secrets, --env renames a variable, --file writes\n" +
  "  a secret to a new mode-0600 file (instead of the environment) that is removed when the command exits.\n" +
  "CHIPOTLE_USAGE_API_KEY overrides the config's scoped billing key.\n" +
  "KEYCHAIN_SKIP_ATTESTATION=1 disables the TEE attestation check (development only).\n";
const attestationOptions = async (litApiUrl) =>
  process.env.KEYCHAIN_SKIP_ATTESTATION === "1"
    ? { attestation: false }
    : { tlsCertificateSha256: await peerCertificateSha256(litApiUrl) };
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
  } else if (command === "actions" && args.length === 0) {
    for (const action of Object.values(ACTIONS)) {
      if (action.deprecated) continue;
      process.stdout.write(
        `${action.id}\n  ${action.name} (${action.kind === "use" ? action.operation : "get"}${action.tier === "community" ? ", community" : ""})\n  ${action.description}\n` +
          (action.kind === "use" && action.input
            ? `  input: ${Object.entries(action.input.properties)
                .map(
                  ([k, v]) =>
                    `${k}${(action.input.required ?? []).includes(k) ? "" : "?"}: ${v.type}`,
                )
                .join(", ")}\n`
            : ""),
      );
    }
  } else if (
    (["get", "stripe-balance"].includes(command) && args.length === 3) ||
    (command === "use" && (args.length === 3 || args.length === 4))
  ) {
    const [identityFile, configFile, name, rawInput] = args;
    let input;
    if (rawInput !== undefined) {
      try {
        input = JSON.parse(rawInput);
      } catch (error) {
        throw new Error(`json-input must be a JSON object: ${error.message}`);
      }
    }
    const identity = await readJson(identityFile);
    assertAgentIdentity(identity);
    const config = await readJson(configFile);
    assertAgentConfig(config);
    const client = new Keychain(identity.privateKey, config, {
      usageApiKey: process.env.CHIPOTLE_USAGE_API_KEY,
      ...(await attestationOptions(config.litApiUrl)),
    });
    try {
      const result =
        command === "get"
          ? await client.get(name)
          : await client.use(name, input);
      process.stdout.write(
        (typeof result === "string" ? result : JSON.stringify(result)) + "\n",
      );
    } finally {
      client.destroy();
    }
  } else if (command === "run") {
    const { parseRunArgs, runWithSecrets } = await import("./run.mjs");
    const options = parseRunArgs(args);
    const identity = await readJson(options.identityFile);
    assertAgentIdentity(identity);
    const config = await readJson(options.configFile);
    assertAgentConfig(config);
    const client = new Keychain(identity.privateKey, config, {
      usageApiKey: process.env.CHIPOTLE_USAGE_API_KEY,
      ...(await attestationOptions(config.litApiUrl)),
    });
    try {
      process.exitCode = await runWithSecrets(client, options, {
        spawn,
        env: process.env,
        stderr: process.stderr,
        process,
      });
    } finally {
      client.destroy();
    }
  } else if (command === "attest" && args.length <= 1) {
    const { verifyAttestation, ATTESTED_ORIGINS, DEFAULT_LIT_API_URL } =
      await import("./dist/index.js");
    const url = args[0] ?? DEFAULT_LIT_API_URL;
    const policy = ATTESTED_ORIGINS[url];
    if (!policy) throw new Error(`No attestation policy is pinned for ${url}`);
    const report = await verifyAttestation(url, policy, {
      tlsCertificateSha256: await peerCertificateSha256(url),
    });
    process.stdout.write(JSON.stringify(report, null, 2) + "\n");
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
