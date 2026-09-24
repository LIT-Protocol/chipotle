// `keychain run`: decrypt export-release secrets and hand them to a child
// process as environment variables or mode-0600 files that live only as long
// as the child. Nothing is written to stdout by the CLI itself; the child owns
// stdio. Modelled on Bitwarden's `bws run`.
import { constants } from "node:os";
import { open, unlink } from "node:fs/promises";
import { resolve } from "node:path";
const ENV_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;

/**
 * Parses `run` arguments after the command word:
 *   <identity-file> <config-file> [--only A,B] [--env SECRET=ENV_VAR]... [--file SECRET=PATH]... -- <command> [args...]
 */
export function parseRunArgs(args) {
  const separator = args.indexOf("--");
  if (separator === -1)
    throw new Error("run needs `--` followed by the command to execute");
  const [identityFile, ...rest] = args.slice(0, separator);
  const configFile =
    rest[0] && !rest[0].startsWith("--") ? rest.shift() : undefined;
  const flags = rest;
  const command = args.slice(separator + 1);
  if (!identityFile) throw new Error("run needs <identity-file> before `--`");
  if (command.length === 0) throw new Error("run needs a command after `--`");
  let only = null;
  const rename = {};
  const files = {};
  for (let i = 0; i < flags.length; i++) {
    const flag = flags[i];
    const value = () => {
      const next = flags[++i];
      if (next === undefined || next.startsWith("--"))
        throw new Error(`${flag} needs a value`);
      return next;
    };
    if (flag === "--only") {
      only = (only ?? []).concat(
        value()
          .split(",")
          .map((name) => name.trim())
          .filter(Boolean),
      );
    } else if (flag === "--env") {
      const pair = value();
      const eq = pair.indexOf("=");
      if (eq <= 0 || eq === pair.length - 1)
        throw new Error(`--env expects SECRET_NAME=ENV_VAR, got ${pair}`);
      const secret = pair.slice(0, eq);
      const envVar = pair.slice(eq + 1);
      if (!ENV_NAME.test(envVar))
        throw new Error(`${envVar} is not a valid environment variable name`);
      rename[secret] = envVar;
    } else if (flag === "--file") {
      const pair = value();
      const eq = pair.indexOf("=");
      if (eq <= 0 || eq === pair.length - 1)
        throw new Error(`--file expects SECRET_NAME=PATH, got ${pair}`);
      const secret = pair.slice(0, eq);
      const path = pair.slice(eq + 1);
      if (path.endsWith("/"))
        throw new Error(`--file path for ${secret} must name a file`);
      if (files[secret] !== undefined)
        throw new Error(`--file given twice for "${secret}"`);
      files[secret] = path;
    } else {
      throw new Error(`Unknown run option ${flag}`);
    }
  }
  if (only !== null && only.length === 0)
    throw new Error("--only needs at least one secret name");
  return { identityFile, configFile, only, rename, files, command };
}

/**
 * Decides where each secret goes. A secret named by --file is written to that
 * path and stays out of the environment unless --env names it too; every other
 * export-release secret becomes a variable named after it.
 * Returns { plan: [{ name, envVar?, file? }], skipped: [name] }.
 */
export function planInjection(list, { only, rename, files = {} }) {
  const byName = new Map();
  for (const secret of list) {
    if (byName.has(secret.name)) byName.set(secret.name, null);
    else byName.set(secret.name, secret);
    if (secret.id) byName.set(secret.id, secret);
  }
  const wanted = only ?? list.map((secret) => secret.name);
  for (const name of [...Object.keys(rename), ...Object.keys(files)]) {
    if (!byName.has(name)) throw new Error(`Unknown secret "${name}"`);
    if (only && !only.includes(name))
      throw new Error(`"${name}" is mapped but not listed under --only`);
  }
  const plan = [];
  const skipped = [];
  const usedVars = new Map();
  const usedPaths = new Map();
  for (const name of wanted) {
    const secret = byName.get(name);
    if (secret === null)
      throw new Error(
        `Secret name "${name}" is ambiguous; use vaultId/secretId and --env to choose its variable`,
      );
    if (!secret) throw new Error(`Unknown secret "${name}"`);
    if (secret.operation !== "get") {
      if (only)
        throw new Error(
          `"${name}" is a "use inside Lit" secret (${secret.operation}); it has no value to inject. Use \`keychain use\`.`,
        );
      skipped.push(name);
      continue;
    }
    const entry = { name };
    if (files[name] !== undefined) {
      const path = resolve(files[name]);
      const clash = usedPaths.get(path);
      if (clash !== undefined)
        throw new Error(`Secrets "${clash}" and "${name}" both map to ${path}`);
      usedPaths.set(path, name);
      entry.file = path;
    }
    if (files[name] === undefined || rename[name] !== undefined) {
      const envVar = rename[name] ?? name;
      if (!ENV_NAME.test(envVar))
        throw new Error(
          `Secret "${name}" is not a valid environment variable name; map it with --env ${name}=SOME_NAME or --file ${name}=PATH`,
        );
      const clash = usedVars.get(envVar);
      if (clash !== undefined && clash !== name)
        throw new Error(
          `Secrets "${clash}" and "${name}" both map to ${envVar}`,
        );
      usedVars.set(envVar, name);
      entry.envVar = envVar;
    }
    plan.push(entry);
  }
  if (plan.length === 0)
    throw new Error(
      only
        ? "Nothing to inject"
        : 'This config has no export-release secrets to inject; every secret is "use inside Lit"',
    );
  return { plan, skipped };
}

/**
 * Fetches the planned secrets through `client.get`, writes any --file targets
 * (mode 0600, never overwriting), spawns the command with the rest in its
 * environment, forwards termination signals, removes the files once the child
 * exits, and resolves to the exit code the CLI should use. `client.destroy()`
 * is called once the child has started so the agent key does not outlive the
 * handoff.
 */
export async function runWithSecrets(
  client,
  { only, rename, files, command },
  { spawn, env, stderr, process: proc },
) {
  const { plan, skipped } = planInjection(await client.list(), {
    only,
    rename,
    files,
  });
  for (const name of skipped)
    stderr.write(
      `Keychain: skipping "${name}" (use inside Lit only, no value to inject)\n`,
    );
  const values = await Promise.all(plan.map(({ name }) => client.get(name)));
  const written = [];
  try {
    for (const [i, { file }] of plan.entries()) {
      if (file === undefined) continue;
      let handle;
      try {
        handle = await open(file, "wx", 0o600);
      } catch (error) {
        throw new Error(
          error.code === "EEXIST"
            ? `${file} already exists; run will not overwrite it`
            : `Cannot create ${file}: ${error.message}`,
        );
      }
      written.push(file);
      try {
        await handle.writeFile(values[i], "utf8");
      } finally {
        await handle.close();
      }
    }
    const childEnv = { ...env };
    plan.forEach(({ envVar }, i) => {
      if (envVar !== undefined) childEnv[envVar] = values[i];
    });
    values.fill("");
    const [file, ...args] = command;
    const child = spawn(file, args, { env: childEnv, stdio: "inherit" });
    for (const key of Object.keys(childEnv)) delete childEnv[key];
    client.destroy();
    const signals = ["SIGINT", "SIGTERM", "SIGHUP"];
    const forward = signals.map((signal) => {
      const handler = () => child.kill(signal);
      proc.on(signal, handler);
      return [signal, handler];
    });
    try {
      return await new Promise((resolve, reject) => {
        child.once("error", (error) =>
          reject(new Error(`Cannot start ${file}: ${error.message}`)),
        );
        child.once("exit", (code, signal) => {
          if (signal) {
            // Mirror the shell convention so callers see why the child stopped.
            stderr.write(`Keychain: ${file} terminated by ${signal}\n`);
            resolve(128 + (constants.signals[signal] ?? 0));
          } else resolve(code ?? 1);
        });
      });
    } finally {
      for (const [signal, handler] of forward) proc.off(signal, handler);
    }
  } finally {
    values.fill("");
    for (const file of written) {
      try {
        await unlink(file);
      } catch (error) {
        if (error.code !== "ENOENT")
          stderr.write(
            `Keychain: could not remove ${file}: ${error.message}\n`,
          );
      }
    }
  }
}
