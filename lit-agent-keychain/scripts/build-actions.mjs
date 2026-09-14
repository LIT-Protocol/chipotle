import { build } from "esbuild";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
await mkdir("generated", { recursive: true });
const release = {};
const templates = {};
await writeFile(
  "generated/discovery.ts",
  "export default " +
    JSON.stringify(await readFile("actions/public-key.js", "utf8")) +
    ";\n",
);
for (const name of ["authority", "export", "stripe-balance"]) {
  const { outputFiles } = await build({
    entryPoints: [`actions/${name}.ts`],
    bundle: true,
    write: false,
    format: "iife",
    globalName: "KeychainAction",
    platform: "browser",
    target: "es2022",
    minify: true,
    legalComments: "none",
    charset: "ascii",
    plugins: [
      {
        name: "native-webcrypto",
        setup(build) {
          build.onResolve({ filter: /^(node:)?crypto$/ }, () => ({
            path: "crypto",
            namespace: "native-webcrypto",
          }));
          build.onLoad({ filter: /.*/, namespace: "native-webcrypto" }, () => ({
            contents: "export const webcrypto = globalThis.crypto;",
            loader: "js",
          }));
        },
      },
    ],
  });
  const code = outputFiles[0].text;
  if (/import\s*\(/.test(code))
    throw new Error("Action bundle has a dynamic import");
  await writeFile(`generated/${name}.js`, code);
  templates[name] = code;
  release[name] = createHash("sha256").update(code).digest("hex");
}
await writeFile(
  "generated/release.json",
  JSON.stringify({ protocol: 2, templates: release }, null, 2) + "\n",
);
await writeFile(
  "generated/templates.ts",
  "export default " + JSON.stringify(templates) + ";\n",
);
