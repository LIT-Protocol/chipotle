import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
export default defineConfig({
  root: import.meta.dirname,
  plugins: [
    react(),
    {
      name: "isolated-owner-app-fixture",
      enforce: "pre",
      resolveId(id, importer) {
        if (id === "/@@app")
          return new URL("../web/src/main.tsx", import.meta.url).pathname;
        if (id === "./identities.ts" && importer?.endsWith("/web/src/main.tsx"))
          return new URL("./fixtures/owner-app-identities.ts", import.meta.url)
            .pathname;
      },
    },
  ],
  server: {
    host: "127.0.0.1",
    port: 55449,
    strictPort: true,
    fs: { allow: [new URL("..", import.meta.url).pathname] },
  },
});
