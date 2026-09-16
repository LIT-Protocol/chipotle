import { mkdtemp, rm } from "node:fs/promises";
import { rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { build, preview } from "vite";
import react from "@vitejs/plugin-react";

const root = path.resolve(import.meta.dirname, "../web");

// A mocked blank document cannot participate in Vite's HMR/reload protocol.
// Build both the real UI and its identity entry before listening, rather than
// exposing dev-server modules while dependency optimization can invalidate them.
// No shared dev cache, web/dist output, warmup requests, sleeps or import retries.
export async function startPasskeyServer(port: number) {
  const outDir = await mkdtemp(path.join(tmpdir(), "keychain-passkey-"));
  // Vite's own SIGTERM handler calls process.exit(), so an async signal
  // cleanup can be cut short. The exit fallback must be synchronous.
  const cleanup = () => rmSync(outDir, { recursive: true, force: true });
  process.once("exit", cleanup);
  try {
    await build({
      configFile: false,
      root,
      plugins: [react()],
      logLevel: "error",
      build: {
        outDir,
        emptyOutDir: true,
        sourcemap: true,
        rollupOptions: {
          input: {
            app: path.join(root, "index.html"),
            identities: path.join(root, "src/identities.ts"),
          },
          // The test-only entry must retain the real module's public exports,
          // even when the app itself does not use every exported function.
          preserveEntrySignatures: "strict",
          output: { entryFileNames: "[name].js" },
        },
      },
    });
    const server = await preview({
      configFile: false,
      root,
      build: { outDir },
      preview: { host: "127.0.0.1", port, strictPort: true },
    });
    return {
      outDir,
      origin: server.resolvedUrls!.local[0].replace(/\/$/, ""),
      async close() {
        try {
          await server.close();
        } finally {
          await rm(outDir, { recursive: true, force: true });
          process.off("exit", cleanup);
        }
      },
    };
  } catch (error) {
    await rm(outDir, { recursive: true, force: true });
    process.off("exit", cleanup);
    throw error;
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  const server = await startPasskeyServer(55449);
  console.log(`Passkey test build ready at ${server.origin}`);
  for (const signal of ["SIGINT", "SIGTERM"] as const) {
    process.once(signal, async () => {
      await server.close();
      process.exit(0);
    });
  }
}
