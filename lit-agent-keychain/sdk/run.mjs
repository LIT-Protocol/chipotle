// `keychain run`: decrypt export-release secrets and hand them to a child
// process as environment variables. Nothing is written to stdout by the CLI
// itself; the child owns stdio. Modelled on Bitwarden's `bws run`.
import { constants } from "node:os";
const ENV_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;

/**
 * Parses `run` arguments after the command word:
 *   <identity-file> <config-file> [--only A,B] [--env SECRET=ENV_VAR]... -- <command> [args...]
 */
export function parseRunArgs(args) {
  const separator = args.indexOf("--");
  if (separator === -1)
    throw new Error("run needs `--` followed by the command to execute");
  const [identityFile, configFile, ...flags] = args.slice(0, separator);
  const command = args.slice(separator + 1);
  if (!identityFile || !configFile)
    throw new Error("run needs <identity-file> <config-file> before `--`");
  if (command.length === 0) throw new Error("run needs a command after `--`");
  let only = null;
  const rename = {};
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
    } else {
      throw new Error(`Unknown run option ${flag}`);
    }
  }
  if (only !== null && only.length === 0)
    throw new Error("--only needs at least one secret name");
  return { identityFile, configFile, only, rename, command };
}

/**
 * Decides which secrets to inject and under which variable names.
 * Returns { plan: [{ name, envVar }], skipped: [name] }.
 */
export function planInjection(list, { only, rename }) {
  const byName = new Map(list.map((secret) => [secret.name, secret]));
  for (const name of Object.keys(rename))
    if (!byName.has(name)) throw new Error(`Unknown secret "${name}"`);
  const wanted = only ?? list.map((secret) => secret.name);
  const plan = [];
  const skipped = [];
  const used = new Map();
  for (const name of wanted) {
    const secret = byName.get(name);
    if (!secret) throw new Error(`Unknown secret "${name}"`);
    if (secret.operation !== "get") {
      if (only)
        throw new Error(
          `"${name}" is a "use inside Lit" secret (${secret.operation}); it has no value to inject. Use \`keychain use\`.`,
        );
      skipped.push(name);
      continue;
    }
    const envVar = rename[name] ?? name;
    if (!ENV_NAME.test(envVar))
      throw new Error(
        `Secret "${name}" is not a valid environment variable name; map it with --env ${name}=SOME_NAME`,
      );
    const clash = used.get(envVar);
    if (clash !== undefined && clash !== name)
      throw new Error(`Secrets "${clash}" and "${name}" both map to ${envVar}`);
    used.set(envVar, name);
    plan.push({ name, envVar });
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
 * Fetches the planned secrets through `client.get`, spawns the command with
 * them in its environment, forwards termination signals, and resolves to the
 * exit code the CLI should use. `client.destroy()` is called once the child
 * has started so the agent key does not outlive the handoff.
 */
export async function runWithSecrets(
  client,
  { only, rename, command },
  { spawn, env, stderr, process: proc },
) {
  const { plan, skipped } = planInjection(client.list(), { only, rename });
  for (const name of skipped)
    stderr.write(
      `Keychain: skipping "${name}" (use inside Lit only, no value to inject)\n`,
    );
  const values = await Promise.all(plan.map(({ name }) => client.get(name)));
  const childEnv = { ...env };
  plan.forEach(({ envVar }, i) => {
    childEnv[envVar] = values[i];
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
}
