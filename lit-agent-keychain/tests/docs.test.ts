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
  for (const file of [
    "web/public/llms.txt",
    "web/src/Landing.tsx",
    "web/src/main.tsx",
  ]) {
    assert.doesNotMatch(
      await read(file),
      /lit-actions\/(derived-actions|signed-storage)/,
    );
  }
});
test("the home page points at the developer docs, not an in-page FAQ", async () => {
  const landing = await read("web/src/Landing.tsx");
  assert.match(landing, /developer\.litprotocol\.com/);
  assert.doesNotMatch(landing, /faq-item|landing-section|pricing-card/);
  const docs = JSON.parse(await read("../docs/docs.json"));
  const tab = docs.navigation.tabs.find(
    (t: { tab: string }) => t.tab === "Lit Agent Keychain",
  );
  assert.ok(tab, "docs.json must have a Lit Agent Keychain tab");
  const pages: string[] = [];
  const walk = (items: unknown[]) => {
    for (const item of items)
      typeof item === "string"
        ? pages.push(item)
        : walk((item as { pages: unknown[] }).pages);
  };
  walk(tab.pages);
  for (const page of pages) await read(`../docs/${page}.mdx`);
  for (const path of ["", "/quickstart", "/security"])
    assert.ok(
      landing.includes(
        path ? `\`\${KEYCHAIN_DOCS_URL}${path}\`` : "KEYCHAIN_DOCS_URL",
      ),
      `home page links ${path || "/keychain"}`,
    );
  for (const path of ["quickstart", "security"])
    assert.ok(pages.includes(`keychain/${path}`), `docs tab lists ${path}`);
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
test("every published version pin matches sdk/package.json", async () => {
  const { version } = JSON.parse(await read("sdk/package.json"));
  const { SDK_VERSION, NPX_KEYCHAIN } = await import("../web/src/version.ts");
  assert.equal(SDK_VERSION, version);
  assert.equal(NPX_KEYCHAIN, `npx @lit-protocol/keychain@${version}`);
  const files = [
    "README.md",
    "PROVIDERS.md",
    "SKILL.md",
    "SECURITY.md",
    "BILLING.md",
    "sdk/README.md",
    "web/public/llms.txt",
    "web/src/Landing.tsx",
    "web/src/AddSecret.tsx",
    "web/src/main.tsx",
  ];
  for (const file of files) {
    const text = await read(file);
    for (const [pin, found] of text.matchAll(/keychain@(\d+\.\d+\.\d+)/g))
      assert.equal(found, version, `${file}: ${pin}`);
    if (file.startsWith("web/src/"))
      assert.doesNotMatch(
        text,
        /keychain@\d/,
        `${file} must use NPX_KEYCHAIN from web/src/version.ts`,
      );
  }
});
