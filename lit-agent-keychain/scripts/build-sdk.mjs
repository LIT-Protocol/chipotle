import { build } from "esbuild";
import { mkdir, readdir, readFile, writeFile, rm } from "node:fs/promises";
import { execFileSync } from "node:child_process";
await mkdir("sdk/dist", { recursive: true });
await build({
  entryPoints: ["sdk/src/index.ts"],
  outfile: "sdk/dist/index.js",
  bundle: true,
  platform: "browser",
  format: "esm",
  target: "es2022",
  minify: true,
  sourcemap: true,
});

execFileSync(
  process.execPath,
  [
    "node_modules/typescript/bin/tsc",
    "--declaration",
    "--emitDeclarationOnly",
    "--noEmit",
    "false",
    "--outDir",
    "sdk/dist/types",
    "--rootDir",
    ".",
    "--moduleResolution",
    "bundler",
    "--module",
    "esnext",
    "--target",
    "es2022",
    "--skipLibCheck",
    "--allowImportingTsExtensions",
    "sdk/src/index.ts",
  ],
  { stdio: "inherit" },
);
for (const file of await readdir("sdk/dist/types", { recursive: true })) {
  if (!file.endsWith(".d.ts")) continue;
  const path = "sdk/dist/types/" + file;
  await writeFile(
    path,
    (await readFile(path, "utf8")).replace(
      /(from\s+["'][^"']+)\.ts(["'])/g,
      "$1.js$2",
    ),
  );
}
await rm("sdk/dist/types/generated", { recursive: true, force: true });
await writeFile(
  "sdk/dist/index.d.ts",
  'export * from "./types/sdk/src/index.js";\n',
);
