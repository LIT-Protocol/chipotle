import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import path from "node:path";
import { fixture, json } from "./harness.ts";
import { decode, open, responseContext } from "../protocol/crypto.ts";
import { actionSource } from "../protocol/actions.ts";
import {
  catalogSchema,
  definitionSchema,
  shapeToZod,
  shapeToJsonSchema,
  type UseDefinition,
} from "@lit-protocol/agent-keychain-library/schema";
import { lintActionSource } from "@lit-protocol/agent-keychain-library/lint";
import { boundFetch } from "../actions/secret-common.ts";
import catalog from "../generated/catalog.ts";
import templates from "../generated/templates.ts";

const libraryDir = path.dirname(
  createRequire(import.meta.url).resolve(
    "@lit-protocol/agent-keychain-library/package.json",
  ),
);
const unseal = async (f: any, out: any) =>
  JSON.parse(
    decode(
      await open(
        f.responseKey,
        out.result.payload.sealed,
        responseContext(f.request),
      ),
    ),
  );

test("every catalog entry validates, matches its directory, and is pinned in the lock", async () => {
  assert.ok(catalogSchema.safeParse(catalog).success);
  const dirs = (
    await readdir(path.join(libraryDir, "actions"), { withFileTypes: true })
  )
    .filter((d) => d.isDirectory())
    .map((d) => d.name)
    .sort();
  assert.deepEqual(Object.keys(catalog).sort(), dirs);
  for (const id of dirs) {
    const raw = JSON.parse(
      await readFile(
        path.join(libraryDir, `actions/${id}/action.json`),
        "utf8",
      ),
    );
    const parsed = definitionSchema.parse(raw);
    assert.equal(parsed.id, id);
    if (parsed.kind === "use")
      assert.deepEqual(
        lintActionSource(
          await readFile(
            path.join(libraryDir, `actions/${id}/action.ts`),
            "utf8",
          ),
        ),
        [],
      );
  }
  const lock = JSON.parse(await readFile("actions/catalog.lock.json", "utf8"));
  for (const [id, code] of Object.entries(templates))
    assert.equal(
      lock.templates[id],
      createHash("sha256").update(code).digest("hex"),
      `${id} template bytes drifted from actions/catalog.lock.json`,
    );
  assert.ok(
    Object.values(catalog)
      .filter((d) => d.kind === "use")
      .every((d) => d.id !== "export" && d.operation !== "get"),
  );
});
test("unknown releases cannot produce action source", () => {
  const f = { ...({} as any) };
  assert.throws(
    () =>
      actionSource({
        v: 2,
        network: "test",
        registry: "https://keychain.test",
        vaultId: "0".repeat(64),
        authorityCid: "Qm" + "a".repeat(44),
        secretId: "1".repeat(64),
        release: "not_in_catalog",
      } as any),
    /Unknown action release/,
  );
  void f;
});
test("the static linter rejects direct network, code and runtime access", () => {
  const ok = `import { defineAction } from "../../lib.ts";\nexport default defineAction(async ({ fetchJson }) => fetchJson("https://a.example/x"));\n`;
  assert.deepEqual(lintActionSource(ok), []);
  const bad: [string, RegExp][] = [
    [`fetch("https://evil.test")`, /direct fetch/],
    [`import { z } from "zod";`, /may only import/],
    [`import "../../lib.ts";`, /side-effect imports/],
    [`Lit.Actions.getLitActionPrivateKey()`, /runtime global/],
    [`globalThis.fetch`, /runtime global/],
    [`eval("1")`, /dynamic code/],
    [`await import("x")`, /dynamic import/],
    [`setTimeout(() => {}, 1)`, /timers/],
    [`crypto.subtle.digest`, /crypto/],
    [`new WebSocket("wss://x")`, /browser network API/],
  ];
  for (const [snippet, why] of bad) {
    const problems = lintActionSource(ok + "\n" + snippet + "\n");
    assert.ok(
      problems.some((p) => why.test(p)),
      `${snippet}: ${problems}`,
    );
  }
  // Mentions inside comments do not count; only code does.
  assert.deepEqual(lintActionSource(ok + "// never call fetch( here\n"), []);
});
test("bound fetch permits only HTTPS to declared hosts within the request budget", async () => {
  const definition = catalog.stripe_balance as UseDefinition;
  const seen: string[] = [];
  const realFetch = globalThis.fetch;
  globalThis.fetch = (async (input: any) => {
    seen.push(String(input));
    return json({ ok: true });
  }) as any;
  try {
    const client = boundFetch(definition);
    for (const url of [
      "http://api.stripe.com/v1/balance",
      "https://evil.test/v1/balance",
      "https://api.stripe.com.evil.test/",
      "https://api.stripe.com:8443/v1/balance",
      "https://user:pw@api.stripe.com/v1/balance",
      "ftp://api.stripe.com/",
    ])
      await assert.rejects(client(url), `${url} should be rejected`);
    await assert.rejects(
      client("https://api.stripe.com/x", { method: "TRACE" as any }),
    );
    assert.deepEqual(seen, []);
    assert.equal(
      await client("https://api.stripe.com/v1/balance"),
      '{"ok":true}',
    );
    // maxRequests is 1 for this action.
    await assert.rejects(client("https://api.stripe.com/v1/balance"));
    assert.equal(seen.length, 1);
  } finally {
    globalThis.fetch = realFetch;
  }
});
test("shapes convert to strict validators and closed JSON Schema", () => {
  const shape = (catalog.openai_chat as UseDefinition).input!;
  const v = shapeToZod(shape);
  assert.ok(
    v.safeParse({
      model: "gpt-4o-mini",
      messages: [{ role: "user", content: "hi" }],
    }).success,
  );
  assert.ok(!v.safeParse({ model: "gpt-4o-mini" }).success);
  assert.ok(
    !v.safeParse({ model: "gpt-4o-mini", messages: [], extra: 1 }).success,
  );
  assert.ok(
    !v.safeParse({
      model: "GPT 4",
      messages: [{ role: "user", content: "hi" }],
    }).success,
  );
  assert.ok(
    !v.safeParse({
      model: "gpt-4o-mini",
      messages: [{ role: "tool", content: "hi" }],
    }).success,
  );
  const js = shapeToJsonSchema(shape) as any;
  assert.equal(js.additionalProperties, false);
  assert.equal(js.properties.messages.items.additionalProperties, false);
});
test("a credential that does not match the action's pattern is never used", async () => {
  const f = await fixture("stripe_balance", undefined, {
    secret: "not-a-stripe-key",
  });
  let called = false;
  f.h.extraFetch = async () => {
    called = true;
    return json({});
  };
  assert.deepEqual(await f.h.run(f.manifest, f.params), {
    ok: false,
    error: "access_denied",
  });
  assert.equal(called, false);
});
test("agent input is validated against the manifest before any key derivation", async () => {
  for (const input of [
    { messages: [{ role: "user", content: "hi" }] }, // missing model
    {
      model: "gpt-4o-mini",
      messages: [{ role: "user", content: "hi" }],
      url: "https://evil.test",
    },
    {
      model: "gpt-4o-mini",
      messages: [{ role: "user", content: "x".repeat(5000) }],
    },
  ]) {
    const f = await fixture("openai_chat", undefined, { input });
    assert.deepEqual(await f.h.run(f.manifest, f.params), {
      ok: false,
      error: "access_denied",
    });
    assert.equal(f.h.privateKeyCalls, 0);
  }
  // Export releases accept no input at all.
  const e = await fixture("export", undefined, { input: { anything: 1 } });
  assert.equal((await e.h.run(e.manifest, e.params)).ok, false);
  assert.equal(e.h.privateKeyCalls, 0);
});
test("OpenAI chat action forwards only the validated prompt and returns a projected reply", async () => {
  const f = await fixture("openai_chat", undefined, {
    input: {
      model: "gpt-4o-mini",
      messages: [{ role: "user", content: "Say hi" }],
      maxTokens: 16,
    },
  });
  f.h.extraFetch = async (input, init) => {
    assert.equal(String(input), "https://api.openai.com/v1/chat/completions");
    assert.equal(init?.method, "POST");
    assert.equal((init?.headers as any).Authorization, `Bearer ${f.secret}`);
    assert.deepEqual(JSON.parse(String(init?.body)), {
      model: "gpt-4o-mini",
      messages: [{ role: "user", content: "Say hi" }],
      max_tokens: 16,
      n: 1,
      stream: false,
    });
    return json({
      id: "chatcmpl-1",
      model: "gpt-4o-mini-2024",
      choices: [
        {
          index: 0,
          message: { role: "assistant", content: "hi" },
          finish_reason: "stop",
        },
      ],
      usage: {
        prompt_tokens: 3,
        completion_tokens: 1,
        total_tokens: 4,
        secret: f.secret,
      },
    });
  };
  const out = await f.h.run(f.manifest, f.params);
  assert.equal(out.ok, true);
  assert.deepEqual(await unseal(f, out), {
    content: "hi",
    finishReason: "stop",
    model: "gpt-4o-mini-2024",
    usage: { promptTokens: 3, completionTokens: 1 },
  });
  assert.ok(!JSON.stringify(out).includes(f.secret));
});
test("results that violate the declared output shape are denied, not returned", async () => {
  const f = await fixture("openai_chat", undefined, {
    input: {
      model: "gpt-4o-mini",
      messages: [{ role: "user", content: "hi" }],
    },
  });
  f.h.extraFetch = async () =>
    json({ choices: [{ message: { content: { nested: "object" } } }] });
  assert.deepEqual(await f.h.run(f.manifest, f.params), {
    ok: false,
    error: "access_denied",
  });
});
test("GitHub read action encodes the route and returns decoded text only", async () => {
  const f = await fixture("github_read_file", undefined, {
    input: {
      owner: "LIT-Protocol",
      repo: "chipotle",
      path: "docs/a&b/README.md",
      ref: "main",
    },
  });
  const text = "# Hello ☃\n";
  f.h.extraFetch = async (input, init) => {
    assert.equal(
      String(input),
      "https://api.github.com/repos/LIT-Protocol/chipotle/contents/docs/a%26b/README.md?ref=main",
    );
    assert.equal((init?.headers as any).Authorization, `Bearer ${f.secret}`);
    return json({
      type: "file",
      path: "docs/a&b/README.md",
      sha: "a".repeat(40),
      size: Buffer.byteLength(text),
      encoding: "base64",
      content: Buffer.from(text).toString("base64"),
      html_url: "https://github.com/x",
    });
  };
  const out = await f.h.run(f.manifest, f.params);
  assert.equal(out.ok, true);
  assert.deepEqual(await unseal(f, out), {
    path: "docs/a&b/README.md",
    sha: "a".repeat(40),
    size: Buffer.byteLength(text),
    content: text,
    truncated: false,
  });
  for (const path of ["../secrets", "a/../b", "/etc", "a//b", "a?x=1"]) {
    const g = await fixture("github_read_file", undefined, {
      input: { owner: "o", repo: "r", path },
    });
    assert.equal((await g.h.run(g.manifest, g.params)).ok, false);
    assert.equal(
      g.h.privateKeyCalls,
      0,
      `${path} should fail input validation`,
    );
  }
});
test("Slack post action treats ok:false as denial and returns only channel and ts", async () => {
  const f = await fixture("slack_post_message", undefined, {
    input: {
      channel: "C0123ABC",
      text: "deploy finished",
      threadTs: "1700000000.000100",
    },
  });
  f.h.extraFetch = async (input, init) => {
    assert.equal(String(input), "https://slack.com/api/chat.postMessage");
    assert.deepEqual(JSON.parse(String(init?.body)), {
      channel: "C0123ABC",
      text: "deploy finished",
      thread_ts: "1700000000.000100",
    });
    return json({
      ok: true,
      channel: "C0123ABC",
      ts: "1700000001.000200",
      message: { text: f.secret },
    });
  };
  const out = await f.h.run(f.manifest, f.params);
  assert.equal(out.ok, true);
  assert.deepEqual(await unseal(f, out), {
    channel: "C0123ABC",
    ts: "1700000001.000200",
  });
  assert.ok(!JSON.stringify(out).includes(f.secret));
  const g = await fixture("slack_post_message", undefined, {
    input: { channel: "C0123ABC", text: "x" },
  });
  g.h.extraFetch = async () => json({ ok: false, error: "channel_not_found" });
  assert.deepEqual(await g.h.run(g.manifest, g.params), {
    ok: false,
    error: "access_denied",
  });
});
