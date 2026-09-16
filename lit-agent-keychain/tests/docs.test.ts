import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { build, createServer, preview } from "vite";
import {
  shapeToZod,
  type UseDefinition,
} from "@lit-protocol/agent-keychain-library/schema";
import catalog from "../generated/catalog.ts";
import { fixture, json } from "./harness.ts";

const read = (p: string) =>
  readFile(new URL(`../${p}`, import.meta.url), "utf8");
test("public owner guide is served as markdown, including query strings", async () => {
  const server = await createServer({
    configFile: "web/vite.config.ts",
    optimizeDeps: { noDiscovery: true, include: [] },
    server: { port: 0 },
  });
  await server.listen();
  try {
    for (const suffix of ["", "?from=skill"]) {
      const response = await fetch(
        server.resolvedUrls!.local[0] + "README.md" + suffix,
      );
      assert.equal(response.status, 200);
      assert.match(response.headers.get("content-type")!, /text\/markdown/);
      assert.equal(await response.text(), await read("README.md"));
    }
  } finally {
    await server.close();
  }
});
test("built public docs are fetchable and local markdown links resolve", async () => {
  await build({ configFile: "web/vite.config.ts", logLevel: "error" });
  const server = await preview({
    configFile: "web/vite.config.ts",
    preview: { port: 0 },
  });
  try {
    const origin = server.resolvedUrls!.local[0];
    const docs = [
      "README.md",
      "PROVIDERS.md",
      "SKILL.md",
      "SECURITY.md",
      "BILLING.md",
      "ADVERSARIAL_REVIEW.md",
      "sdk/README.md",
    ];
    for (const doc of docs) {
      const response = await fetch(new URL(doc, origin));
      assert.equal(response.status, 200, doc);
      const text = await response.text();
      assert.equal(
        text,
        await read(doc),
        `${doc} must be the exact source, not the SPA shell`,
      );
      for (const match of text.matchAll(/\]\(([^)]+\.md(?:#[^)]*)?)\)/g)) {
        const target = new URL(match[1], new URL(doc, origin));
        if (target.origin !== new URL(origin).origin) continue;
        const linked = await fetch(target);
        assert.equal(linked.status, 200, `${doc} -> ${target.pathname}`);
        assert.doesNotMatch(
          await linked.text(),
          /<!doctype html>/i,
          target.pathname,
        );
      }
    }
  } finally {
    await new Promise<void>((resolve, reject) =>
      server.httpServer.close((error) => (error ? reject(error) : resolve())),
    );
  }
});

test("owner action guidance links provider setup and explains exposure", async () => {
  const { createElement } = await import("react");
  const { renderToStaticMarkup } = await import("react-dom/server");
  const { ActionDocs } = await import("../web/src/AddSecret.tsx");
  const html = renderToStaticMarkup(
    createElement(ActionDocs, {
      action: catalog.supabase_tables as UseDefinition,
      secretName: "SUPABASE_TABLE_ACCESS",
    }),
  );
  assert.match(html, /href="\/PROVIDERS.md"/);
  assert.match(html, /provider receives the credential over TLS/);
  assert.match(html, /Do not blindly retry writes/);
});

test("public docs do not advertise known missing platform pages", async () => {
  for (const file of ["web/public/llms.txt", "web/src/Landing.tsx"]) {
    assert.doesNotMatch(
      await read(file),
      /lit-actions\/(derived-actions|signed-storage)/,
    );
  }
});
test("documented Supabase credential passes the actual pinned parser and input schema", async () => {
  const docs = await read("PROVIDERS.md");
  const credential = JSON.parse(
    docs.match(/<!-- supabase-credential -->\s*```json\n([\s\S]*?)\n```/)![1],
  );
  const input = JSON.parse(
    docs.match(/<!-- supabase-select -->\s*```json\n([\s\S]*?)\n```/)![1],
  );
  shapeToZod((catalog.supabase_tables as UseDefinition).input!).parse(input);
  const f = await fixture("supabase_tables", undefined, {
    secret: JSON.stringify(credential),
    input,
  });
  let calls = 0;
  f.h.extraFetch = async (url) => {
    calls++;
    assert.equal(new URL(String(url)).host, `${credential.ref}.supabase.co`);
    return json([]);
  };
  assert.equal((await f.h.run(f.manifest, f.params)).ok, true);
  assert.equal(calls, 1);
  const bad = await fixture("supabase_tables", undefined, {
    secret: JSON.stringify({ ...credential, unexpected: true }),
    input,
  });
  bad.h.extraFetch = async () => {
    assert.fail("invalid credential must not reach provider");
  };
  assert.equal((await bad.h.run(bad.manifest, bad.params)).ok, false);
});
