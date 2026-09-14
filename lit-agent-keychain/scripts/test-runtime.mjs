import { spawn } from "node:child_process";
import path from "node:path";
const root = path.resolve(import.meta.dirname, "..");
const cargo = [
  "+1.91",
  "test",
  "--manifest-path",
  path.resolve(root, "../lit-actions/Cargo.toml"),
  "-p",
  "lit-actions-tests",
  "--test",
  "integration",
];
function run(command, args, env = process.env) {
  return new Promise((resolve, reject) => {
    const p = spawn(command, args, { cwd: root, env, stdio: "inherit" });
    p.on("error", reject);
    p.on("exit", (code) =>
      code === 0 ? resolve() : reject(new Error(`${command} exited ${code}`)),
    );
  });
}
await run("cargo", [...cargo, "--no-run"]);
const fixture = spawn(
  process.execPath,
  ["--import", "tsx", "scripts/runtime-vector.ts"],
  { cwd: root, stdio: ["ignore", "pipe", "inherit"] },
);
try {
  await new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error("Runtime fixture startup timed out")),
      30000,
    );
    timer.unref();
    fixture.stdout.on("data", (data) => {
      if (String(data).includes("Runtime fixture ready")) {
        clearTimeout(timer);
        resolve();
      }
    });
    fixture.on("error", reject);
    fixture.on("exit", () => reject(new Error("Fixture server stopped")));
  });
  await run(
    "cargo",
    [...cargo, "keychain_encrypted_release", "--", "--ignored", "--nocapture"],
    {
      ...process.env,
      KEYCHAIN_RUNTIME_VECTOR: path.join(root, "generated/runtime-vector.json"),
    },
  );
  await run(process.execPath, [
    "--import",
    "tsx",
    "scripts/verify-runtime-response.ts",
  ]);
} finally {
  fixture.kill("SIGTERM");
}
