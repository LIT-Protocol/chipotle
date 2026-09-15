import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { copyFileSync, mkdirSync, readFileSync } from "node:fs";

const root = path.resolve(import.meta.dirname);
const pkg = path.resolve(root, "..");
// Markdown from the package root published alongside the app so agents can
// fetch docs from the deployed origin (linked from the homepage and llms.txt).
const publishedDocs: [string, string][] = [
  ["/SKILL.md", "SKILL.md"],
  ["/SECURITY.md", "SECURITY.md"],
  ["/sdk/README.md", "sdk/README.md"],
];
function publishDocs(): Plugin {
  return {
    name: "keychain-publish-docs",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const hit = publishedDocs.find(
          ([url]) => (req.url || "").split("?")[0] === url,
        );
        if (!hit) return next();
        res.setHeader("content-type", "text/markdown; charset=utf-8");
        res.end(readFileSync(path.join(pkg, hit[1])));
      });
    },
    closeBundle() {
      for (const [url, source] of publishedDocs) {
        const dest = path.join(root, "dist", url);
        mkdirSync(path.dirname(dest), { recursive: true });
        copyFileSync(path.join(pkg, source), dest);
      }
    },
  };
}
export default defineConfig({
  root,
  plugins: [react(), publishDocs()],
  build: { outDir: "dist", emptyOutDir: true, sourcemap: true },
  server: {
    host: "127.0.0.1",
    port: 5173,
    proxy: {
      "/api": "http://127.0.0.1:8000",
      "/auth": "http://127.0.0.1:8000",
    },
  },
});
