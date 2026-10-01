import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { operate } from "../dist/crypto.js";
import { KDF } from "../../lit-static/dapps/dashboard/password-protocol.js";
let mf, db;
const origin = "https://dashboard.test",
  password = "correct horse battery staple",
  key = Buffer.alloc(32, 7).toString("base64"),
  address = "0x" + "12".repeat(20);
before(async () => {
  mf = new Miniflare(
    convertV4MiniflareOptions({
      workers: [
        {
          modules: true,
          scriptPath: "dist/worker.js",
          compatibilityDate: "2025-10-11",
          d1Databases: ["DB"],
          bindings: {
            ENVIRONMENT: "test",
            DASHBOARD_URL: origin + "/dapps/dashboard/",
            AUTH_SECRET: "test-only-secret-not-for-production-12345",
            RESEND_API_KEY: "test",
            MAIL_FROM: "test@example.com",
          },
          outboundService: () => Response.json({ id: "mock-email" }),
        },
      ],
    }),
  );
  db = await mf.getD1Database("DB");
  for (const statement of (await readFile("migrations/0001_auth.sql", "utf8"))
    .split(";")
    .map((s) => s.trim())
    .filter(Boolean))
    await db.prepare(statement).run();
});
after(async () => {
  await mf?.dispose();
});
let ip = 1;
async function call(path, body = {}, client = {}, method = "POST", extra = {}) {
  const response = await mf.dispatchFetch("https://auth.test/auth/v1/" + path, {
    method,
    headers: {
      Origin: origin,
      "X-Chipotle-Auth": "1",
      "Content-Type": "application/json",
      "CF-Connecting-IP": `192.0.2.${ip++}`,
      ...(client.cookie ? { Cookie: client.cookie } : {}),
      ...(client.csrf ? { "X-CSRF-Token": client.csrf } : {}),
      ...extra,
    },
    ...(method !== "GET"
      ? { body: JSON.stringify({ ...client.binding, ...body }) }
      : {}),
  });
  return {
    status: response.status,
    data: await response.json(),
    cookie: response.headers.get("set-cookie")?.split(";")[0],
  };
}
async function signup(emailAddress) {
  const token = Buffer.from(
      crypto.getRandomValues(new Uint8Array(32)),
    ).toString("hex"),
    hash = Buffer.from(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(token)),
    ).toString("hex");
  await db
    .prepare(
      "INSERT INTO auth_tokens(hash,email,purpose,expires_at) VALUES (?,?,'signup',?)",
    )
    .bind(hash, emailAddress, Math.floor(Date.now() / 1000) + 1000)
    .run();
  const verified = await call("signup/verify", { token });
  assert.equal(verified.status, 200, JSON.stringify(verified.data));
  const p = verified.data.parameters,
    client = {
      cookie: verified.cookie,
      csrf: verified.data.csrf,
      binding: { id: p.id, version: p.version },
    };
  const derived = await operate({
    operation: "derive",
    password,
    parameters: p,
  });
  assert.equal(
    (
      await call(
        "signup/credentials",
        { authSecret: derived.authSecret },
        client,
      )
    ).status,
    200,
  );
  const begin = await call(
    "signup/begin",
    { authSecret: derived.authSecret },
    client,
  );
  assert.equal(begin.status, 200);
  return {
    p,
    client,
    authSecret: derived.authSecret,
    operation: begin.data.operation,
    token,
  };
}
test("browser crypto round trip, password/AAD/KDF rejection and password re-encryption", async () => {
  const p = {
    format: 1,
    environment: "test",
    id: "11".repeat(16),
    salt: "22".repeat(16),
    version: 1,
    kdf: KDF,
  };
  const encrypted = await operate({
    operation: "encrypt",
    password,
    parameters: p,
    apiKey: key,
    account: address,
  });
  assert.equal(
    (
      await operate({
        operation: "decrypt",
        password,
        parameters: p,
        envelope: encrypted.envelope,
      })
    ).apiKey,
    key,
  );
  await assert.rejects(
    operate({
      operation: "decrypt",
      password: "wrong password here",
      parameters: p,
      envelope: encrypted.envelope,
    }),
    /unlock/,
  );
  await assert.rejects(
    operate({
      operation: "decrypt",
      password,
      parameters: { ...p, kdf: { ...KDF, memory: 2 ** 30 } },
      envelope: encrypted.envelope,
    }),
    /parameters/,
  );
  await assert.rejects(
    operate({
      operation: "decrypt",
      password,
      parameters: p,
      envelope: { ...encrypted.envelope, account: "0x" + "13".repeat(20) },
    }),
    /unlock/,
  );
  const changed = await operate({
    operation: "change",
    password,
    newPassword: "a brand new password phrase",
    parameters: p,
    envelope: encrypted.envelope,
  });
  assert.notEqual(changed.envelope.nonce, encrypted.envelope.nonce);
  assert.notEqual(changed.parameters.salt, p.salt);
  assert.equal(
    (
      await operate({
        operation: "decrypt",
        password: "a brand new password phrase",
        parameters: changed.parameters,
        envelope: changed.envelope,
      })
    ).apiKey,
    key,
  );
});
test("signup replay, creation claim, upload idempotency, CSRF, login and atomic concurrent password changes", async () => {
  const user = await signup("alice@example.com");
  assert.equal(
    (await call("signup/verify", { token: user.token })).status,
    400,
  );
  assert.equal(
    (await call("signup/begin", { authSecret: user.authSecret }, user.client))
      .status,
    409,
  );
  // A definitive Lit API rejection lets the same attempt be released and
  // retried under a fresh operation id; the released id can no longer upload.
  assert.equal(
    (
      await call(
        "signup/begin",
        {
          authSecret: user.authSecret,
          retry: true,
          operation: "00".repeat(16),
        },
        user.client,
      )
    ).status,
    409,
  );
  const released = await call(
    "signup/begin",
    { authSecret: user.authSecret, retry: true, operation: user.operation },
    user.client,
  );
  assert.equal(released.status, 200);
  assert.notEqual(released.data.operation, user.operation);
  const staleOperation = user.operation;
  user.operation = released.data.operation;
  const encrypted = await operate({
      operation: "encrypt",
      password,
      parameters: user.p,
      apiKey: key,
      account: address,
    }),
    upload = {
      authSecret: user.authSecret,
      operation: user.operation,
      envelope: encrypted.envelope,
    };
  assert.equal(
    (
      await call(
        "envelope",
        { ...upload, operation: staleOperation },
        user.client,
        "PUT",
      )
    ).status,
    409,
  );
  assert.equal(
    (await call("envelope", upload, { ...user.client, csrf: "bad" }, "PUT"))
      .status,
    403,
  );
  assert.equal(
    (await call("envelope", upload, user.client, "PUT")).status,
    200,
  );
  assert.equal(
    (await call("envelope", upload, user.client, "PUT")).status,
    200,
  );
  assert.equal(
    (
      await call(
        "envelope",
        {
          ...upload,
          envelope: { ...encrypted.envelope, nonce: "ff".repeat(12) },
        },
        user.client,
        "PUT",
      )
    ).status,
    409,
  );
  assert.equal(
    (
      await call("login", {
        email: "alice@example.com",
        authSecret: "aa".repeat(32),
      })
    ).status,
    401,
  );
  const login = await call("login", {
    email: "ALICE@example.com",
    authSecret: user.authSecret,
  });
  assert.equal(login.status, 200);
  const client = {
    cookie: login.cookie,
    csrf: login.data.csrf,
    binding: user.client.binding,
  };
  assert.equal(
    (await call("envelope", {}, client, "GET")).data.envelope.ciphertext,
    encrypted.envelope.ciphertext,
  );
  const changed = await operate({
      operation: "change",
      password,
      newPassword: "another totally new password",
      parameters: user.p,
      envelope: encrypted.envelope,
    }),
    change = {
      oldAuthSecret: user.authSecret,
      authSecret: changed.authSecret,
      envelope: changed.envelope,
    };
  const results = await Promise.all([
    call("password/change", change, client),
    call("password/change", change, client),
  ]);
  assert.equal(
    results.filter((r) => r.status === 200).length,
    1,
    JSON.stringify(results),
  );
  assert.equal((await call("envelope", {}, client, "GET")).status, 401);
  assert.equal(
    (
      await call("login", {
        email: "alice@example.com",
        authSecret: user.authSecret,
      })
    ).status,
    401,
  );
  assert.equal(
    (
      await call("login", {
        email: "alice@example.com",
        authSecret: changed.authSecret,
      })
    ).status,
    200,
  );
  assert.equal(
    (
      await db
        .prepare("SELECT version FROM auth_users WHERE id=?")
        .bind(user.p.id)
        .first()
    ).version,
    2,
  );
});
test("no reset/import, origin checks, synthetic parameters, rate limits and cross-identity mutations", async () => {
  assert.equal((await call("password/forgot")).status, 404);
  assert.equal((await call("link/start")).status, 404);
  assert.equal(
    (
      await call(
        "login/parameters",
        { email: "unknown@example.com" },
        {},
        "POST",
        { Origin: "https://evil.test" },
      )
    ).status,
    403,
  );
  const a = await call("login/parameters", { email: "unknown@example.com" }),
    b = await call("login/parameters", { email: "unknown@example.com" });
  assert.deepEqual(a.data, b.data);
  assert.deepEqual(a.data.kdf, KDF);
  for (let i = 0; i < 3; i++)
    assert.equal(
      (await call("signup/start", { email: "limited@example.com" })).status,
      200,
    );
  assert.equal(
    (await call("signup/start", { email: "limited@example.com" })).status,
    429,
  );
  const user = await signup("bob@example.com");
  assert.equal(
    (
      await call(
        "logout",
        {},
        { ...user.client, binding: { id: "ff".repeat(16), version: 1 } },
      )
    ).status,
    409,
  );
  const u = await db
    .prepare("SELECT * FROM auth_users WHERE id=?")
    .bind(user.p.id)
    .first();
  assert.equal(u.envelope, null);
  assert.equal(u.state, "creating");
  assert.notEqual(u.verifier, user.authSecret);
});
test("email change cannot replace encrypted keys; conflicting email rolls back token consumption", async () => {
  const user = await signup("email-change@example.com");
  const encrypted = await operate({
    operation: "encrypt",
    password,
    parameters: user.p,
    apiKey: key,
    account: "0x" + "56".repeat(20),
  });
  assert.equal(
    (
      await call(
        "envelope",
        {
          authSecret: user.authSecret,
          operation: user.operation,
          envelope: encrypted.envelope,
        },
        user.client,
        "PUT",
      )
    ).status,
    200,
  );
  const login = await call("login", {
    email: "email-change@example.com",
    authSecret: user.authSecret,
  });
  const client = {
    cookie: login.cookie,
    csrf: login.data.csrf,
    binding: user.client.binding,
  };
  async function proof(address) {
    const raw = Buffer.from(
        crypto.getRandomValues(new Uint8Array(32)),
      ).toString("hex"),
      hash = Buffer.from(
        await crypto.subtle.digest("SHA-256", new TextEncoder().encode(raw)),
      ).toString("hex");
    await db
      .prepare(
        "INSERT INTO auth_tokens(hash,email,user_id,version,purpose,expires_at) VALUES (?,?,?,1,'email',?)",
      )
      .bind(hash, address, user.p.id, Math.floor(Date.now() / 1000) + 500)
      .run();
    return { raw, hash };
  }
  const duplicate = await proof("alice@example.com");
  assert.equal(
    (
      await call(
        "email/complete",
        { token: duplicate.raw, authSecret: user.authSecret },
        client,
      )
    ).status,
    409,
  );
  assert.equal(
    (
      await db
        .prepare("SELECT claim FROM auth_tokens WHERE hash=?")
        .bind(duplicate.hash)
        .first()
    ).claim,
    null,
  );
  const valid = await proof("new-email@example.com");
  assert.equal(
    (
      await call(
        "email/complete",
        { token: valid.raw, authSecret: "ff".repeat(32) },
        client,
      )
    ).status,
    401,
  );
  assert.equal(
    (
      await call(
        "email/complete",
        { token: valid.raw, authSecret: user.authSecret },
        client,
      )
    ).status,
    200,
  );
  assert.equal((await call("session", {}, client, "GET")).status, 401);
  assert.equal(
    (
      await call("login", {
        email: "email-change@example.com",
        authSecret: user.authSecret,
      })
    ).status,
    401,
  );
  const next = await call("login", {
    email: "new-email@example.com",
    authSecret: user.authSecret,
  });
  assert.equal(next.status, 200);
  assert.deepEqual(next.data.envelope, encrypted.envelope);
  const nextClient = {
    cookie: next.cookie,
    csrf: next.data.csrf,
    binding: user.client.binding,
  };
  assert.equal(
    (
      await call(
        "email/complete",
        { token: valid.raw, authSecret: user.authSecret },
        nextClient,
      )
    ).status,
    400,
  );
});
test("expired sessions and signup tokens fail closed; fresh verification can resume abandoned uncredentialed signup", async () => {
  const expired = "ee".repeat(32),
    hash = Buffer.from(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(expired)),
    ).toString("hex");
  await db
    .prepare(
      "INSERT INTO auth_tokens(hash,email,purpose,expires_at) VALUES (?,'expired@example.com','signup',1)",
    )
    .bind(hash)
    .run();
  assert.equal((await call("signup/verify", { token: expired })).status, 400);
  const user = await signup("expired-session@example.com");
  await db
    .prepare("UPDATE auth_sessions SET idle_until=1 WHERE user_id=?")
    .bind(user.p.id)
    .run();
  assert.equal((await call("session", {}, user.client, "GET")).status, 401);
  const id = "77".repeat(16);
  await db
    .prepare(
      "INSERT INTO auth_users(id,email,salt,created_at) VALUES (?,'abandoned@example.com',?,1)",
    )
    .bind(id, "88".repeat(16))
    .run();
  assert.equal(
    (await call("signup/start", { email: "abandoned@example.com" })).status,
    200,
  );
  assert.ok(
    await db
      .prepare(
        "SELECT hash FROM auth_tokens WHERE email='abandoned@example.com'",
      )
      .first(),
  );
});
