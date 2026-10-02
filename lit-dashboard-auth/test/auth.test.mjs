// API-level tests against the real lit-payments auth service (HTTP) and its
// Postgres database. Mirrors the browser contract in openapi.json.
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { startAuthService } from "./server.mjs";
import { operate } from "../dist/crypto.js";
import { KDF } from "../../lit-static/dapps/dashboard/password-protocol.js";
let service, db;
const origin = "https://dashboard.test",
  password = "correct horse battery staple",
  key = Buffer.alloc(32, 7).toString("base64"),
  address = "0x" + "12".repeat(20);
const sha256 = async (value) =>
  Buffer.from(
    await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value)),
  ).toString("hex");
const randomHex = (n) => Buffer.from(crypto.getRandomValues(new Uint8Array(n))).toString("hex");
const epoch = () => Math.floor(Date.now() / 1000);
before(async () => {
  service = await startAuthService({ dashboardUrls: [origin + "/dapps/dashboard/"] });
  db = service.db;
});
after(async () => {
  await service?.stop();
});
let ip = 1;
async function call(path, body = {}, client = {}, method = "POST", extra = {}) {
  const response = await fetch(service.url + "/auth/v1/" + path, {
    method,
    headers: {
      Origin: origin,
      "X-Chipotle-Auth": "1",
      "Content-Type": "application/json",
      // Distinct client address per call so the per-IP limit never interferes.
      "X-Forwarded-For": `192.0.2.${(ip++ % 250) + 1}, 198.51.100.${ip % 250}`,
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
    data: await response.json().catch(() => null),
    headers: response.headers,
    cookie: response.headers.getSetCookie()[0]?.split(";")[0],
  };
}
async function insertSignupToken(emailAddress) {
  const token = randomHex(32);
  await db.query(
    "INSERT INTO dashboard_auth_tokens(hash,email,purpose,expires_at) VALUES ($1,$2,'signup',$3)",
    [await sha256(token), emailAddress, epoch() + 1000],
  );
  return token;
}
async function signup(emailAddress) {
  const token = await insertSignupToken(emailAddress);
  const verified = await call("signup/verify", { token });
  assert.equal(verified.status, 200, JSON.stringify(verified.data));
  assert.match(verified.cookie, /^__Host-chipotle_auth=[0-9a-f]{64}$/);
  const p = verified.data.parameters,
    client = {
      cookie: verified.cookie,
      csrf: verified.data.csrf,
      binding: { id: p.id, version: p.version },
    };
  const derived = await operate({ operation: "derive", password, parameters: p });
  assert.equal(
    (await call("signup/credentials", { authSecret: derived.authSecret }, client)).status,
    200,
  );
  const begin = await call("signup/begin", { authSecret: derived.authSecret }, client);
  assert.equal(begin.status, 200);
  return { p, client, authSecret: derived.authSecret, operation: begin.data.operation, token };
}
test("browser crypto round trip, password/AAD/KDF rejection and password re-encryption", async () => {
  const p = {
    format: 1,
    environment: "local",
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
    (await operate({ operation: "decrypt", password, parameters: p, envelope: encrypted.envelope }))
      .apiKey,
    key,
  );
  await assert.rejects(
    operate({ operation: "decrypt", password: "wrong password here", parameters: p, envelope: encrypted.envelope }),
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
test("response headers, gate order and body limits", async () => {
  const health = await fetch(service.url + "/health");
  assert.equal(health.status, 200);
  const missingHeader = await call("login/parameters", { email: "x@example.com" }, {}, "POST", {
    "X-Chipotle-Auth": "0",
  });
  assert.equal(missingHeader.status, 403);
  assert.equal(missingHeader.headers.get("cache-control"), "no-store");
  assert.equal(missingHeader.headers.get("x-content-type-options"), "nosniff");
  assert.equal(missingHeader.headers.get("access-control-allow-origin"), origin);
  assert.equal(missingHeader.headers.get("access-control-allow-credentials"), "true");
  assert.equal(
    (await call("login/parameters", {}, {}, "POST", { "Content-Type": "text/plain" })).status,
    415,
  );
  const big = await fetch(service.url + "/auth/v1/login/parameters", {
    method: "POST",
    headers: { Origin: origin, "X-Chipotle-Auth": "1", "Content-Type": "application/json" },
    body: JSON.stringify({ email: "x@example.com", pad: "a".repeat(9000) }),
  });
  assert.equal(big.status, 413);
  const invalid = await fetch(service.url + "/auth/v1/login/parameters", {
    method: "POST",
    headers: { Origin: origin, "X-Chipotle-Auth": "1", "Content-Type": "application/json" },
    body: "[1,2]",
  });
  assert.equal(invalid.status, 400);
  assert.deepEqual(await invalid.json(), { error: "Invalid JSON." });
  const preflight = await fetch(service.url + "/auth/v1/login", {
    method: "OPTIONS",
    headers: {
      Origin: origin,
      "Access-Control-Request-Method": "POST",
      "Access-Control-Request-Headers": "content-type,x-chipotle-auth,x-csrf-token",
    },
  });
  assert.equal(preflight.status, 204);
  assert.equal(preflight.headers.get("access-control-allow-origin"), origin);
  assert.match(preflight.headers.get("access-control-allow-headers"), /x-chipotle-auth/i);
  assert.equal((await call("signup/start", { email: "not-an-email" })).status, 400);
});
test("signup replay, creation claim, upload idempotency, CSRF, login and atomic concurrent password changes", async () => {
  const user = await signup("alice@example.com");
  assert.equal((await call("signup/verify", { token: user.token })).status, 400);
  assert.equal(
    (await call("signup/begin", { authSecret: user.authSecret }, user.client)).status,
    409,
  );
  // A definitive Lit API rejection lets the same attempt be released and
  // retried under a fresh operation id; the released id can no longer upload.
  assert.equal(
    (
      await call(
        "signup/begin",
        { authSecret: user.authSecret, retry: true, operation: "00".repeat(16) },
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
    upload = { authSecret: user.authSecret, operation: user.operation, envelope: encrypted.envelope };
  assert.equal(
    (await call("envelope", { ...upload, operation: staleOperation }, user.client, "PUT")).status,
    409,
  );
  assert.equal(
    (await call("envelope", upload, { ...user.client, csrf: "bad" }, "PUT")).status,
    403,
  );
  assert.equal(
    (await call("envelope", { ...upload, envelope: { ...encrypted.envelope, ciphertext: "zz" } }, user.client, "PUT")).status,
    400,
  );
  assert.equal((await call("envelope", upload, user.client, "PUT")).status, 200);
  assert.equal((await call("envelope", upload, user.client, "PUT")).status, 200);
  assert.equal(
    (
      await call(
        "envelope",
        { ...upload, envelope: { ...encrypted.envelope, nonce: "ff".repeat(12) } },
        user.client,
        "PUT",
      )
    ).status,
    409,
  );
  assert.equal(
    (await call("login", { email: "alice@example.com", authSecret: "aa".repeat(32) })).status,
    401,
  );
  const login = await call("login", { email: "ALICE@example.com", authSecret: user.authSecret });
  assert.equal(login.status, 200);
  assert.equal(login.data.state, "active");
  const client = { cookie: login.cookie, csrf: login.data.csrf, binding: user.client.binding };
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
    change = { oldAuthSecret: user.authSecret, authSecret: changed.authSecret, envelope: changed.envelope };
  const results = await Promise.all([
    call("password/change", change, client),
    call("password/change", change, client),
  ]);
  assert.equal(results.filter((r) => r.status === 200).length, 1, JSON.stringify(results));
  assert.equal((await call("envelope", {}, client, "GET")).status, 401);
  assert.equal(
    (await call("login", { email: "alice@example.com", authSecret: user.authSecret })).status,
    401,
  );
  const relogin = await call("login", { email: "alice@example.com", authSecret: changed.authSecret });
  assert.equal(relogin.status, 200);
  assert.deepEqual(relogin.data.envelope, changed.envelope);
  assert.equal(relogin.data.parameters.version, 2);
  assert.equal(
    (await db.query("SELECT version FROM dashboard_auth_users WHERE id=$1", [user.p.id])).rows[0]
      .version,
    "2",
  );
  const notice = service.mail.find(
    (m) => m.to === "alice@example.com" && m.subject === "Chipotle password changed",
  );
  assert.ok(notice, "password change notification delivered");
});
test("no reset/import, origin checks, synthetic parameters, rate limits and cross-identity mutations", async () => {
  assert.equal((await call("password/forgot")).status, 404);
  assert.equal((await call("link/start")).status, 404);
  assert.equal((await call("session", {}, {}, "GET")).status, 401);
  assert.equal(
    (
      await call("login/parameters", { email: "unknown@example.com" }, {}, "POST", {
        Origin: "https://evil.test",
      })
    ).status,
    403,
  );
  const a = await call("login/parameters", { email: "unknown@example.com" }),
    b = await call("login/parameters", { email: "unknown@example.com" });
  assert.equal(a.status, 200);
  assert.deepEqual(a.data, b.data);
  assert.deepEqual(a.data.kdf, KDF);
  assert.equal(a.data.environment, "local");
  assert.match(a.data.id, /^[0-9a-f]{32}$/);
  for (let i = 0; i < 3; i++)
    assert.equal((await call("signup/start", { email: "limited@example.com" })).status, 200);
  assert.equal((await call("signup/start", { email: "limited@example.com" })).status, 429);
  const user = await signup("bob@example.com");
  assert.equal(
    (await call("logout", {}, { ...user.client, binding: { id: "ff".repeat(16), version: 1 } }))
      .status,
    409,
  );
  const u = (await db.query("SELECT * FROM dashboard_auth_users WHERE id=$1", [user.p.id])).rows[0];
  assert.equal(u.envelope, null);
  assert.equal(u.state, "creating");
  assert.notEqual(u.verifier, user.authSecret);
  assert.equal((await call("logout", {}, user.client)).status, 200);
  assert.equal((await call("session", {}, user.client, "GET")).status, 401);
});
test("signup/start delivers a verification link through the outbox and ignores existing users", async () => {
  const start = await call("signup/start", { email: "fresh@example.com" });
  assert.equal(start.status, 200);
  let message;
  for (let i = 0; i < 40 && !message; i++) {
    message = service.mail.find((m) => m.to === "fresh@example.com");
    if (!message) await new Promise((r) => setTimeout(r, 100));
  }
  assert.ok(message, "verification email captured");
  assert.equal(message.from, "accounts@example.com");
  assert.ok(message.idempotencyKey);
  const link = message.text.match(/https:\/\/dashboard\.test\/dapps\/dashboard\/#verify=([0-9a-f]{64})&purpose=signup/);
  assert.ok(link, message.text);
  const verified = await call("signup/verify", { token: link[1] });
  assert.equal(verified.status, 200);
  assert.equal(verified.data.email, "fresh@example.com");
  assert.equal(verified.data.state, "verified");
  const before = service.mail.length;
  const existing = await call("signup/start", { email: "alice@example.com" });
  assert.equal(existing.status, 200);
  await new Promise((r) => setTimeout(r, 300));
  assert.equal(service.mail.length, before);
  assert.equal(
    (await db.query("SELECT count(*) FROM dashboard_auth_outbox")).rows[0].count,
    "0",
    "outbox drained",
  );
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
        { authSecret: user.authSecret, operation: user.operation, envelope: encrypted.envelope },
        user.client,
        "PUT",
      )
    ).status,
    200,
  );
  const login = await call("login", { email: "email-change@example.com", authSecret: user.authSecret });
  const client = { cookie: login.cookie, csrf: login.data.csrf, binding: user.client.binding };
  async function proof(address) {
    const raw = randomHex(32),
      hash = await sha256(raw);
    await db.query(
      "INSERT INTO dashboard_auth_tokens(hash,email,user_id,version,purpose,expires_at) VALUES ($1,$2,$3,1,'email',$4)",
      [hash, address, user.p.id, epoch() + 500],
    );
    return { raw, hash };
  }
  const duplicate = await proof("alice@example.com");
  assert.equal(
    (await call("email/complete", { token: duplicate.raw, authSecret: user.authSecret }, client))
      .status,
    409,
  );
  assert.equal(
    (await db.query("SELECT claim FROM dashboard_auth_tokens WHERE hash=$1", [duplicate.hash]))
      .rows[0].claim,
    null,
  );
  const valid = await proof("new-email@example.com");
  assert.equal(
    (await call("email/complete", { token: valid.raw, authSecret: "ff".repeat(32) }, client)).status,
    401,
  );
  assert.equal(
    (await call("email/complete", { token: valid.raw, authSecret: user.authSecret }, client)).status,
    200,
  );
  assert.equal((await call("session", {}, client, "GET")).status, 401);
  assert.equal(
    (await call("login", { email: "email-change@example.com", authSecret: user.authSecret })).status,
    401,
  );
  const next = await call("login", { email: "new-email@example.com", authSecret: user.authSecret });
  assert.equal(next.status, 200);
  assert.deepEqual(next.data.envelope, encrypted.envelope);
  const nextClient = { cookie: next.cookie, csrf: next.data.csrf, binding: user.client.binding };
  assert.equal(
    (await call("email/complete", { token: valid.raw, authSecret: user.authSecret }, nextClient))
      .status,
    400,
  );
  // The notice went to the previous address.
  for (let i = 0; i < 40; i++) {
    if (service.mail.some((m) => m.to === "email-change@example.com" && /email changed/.test(m.subject))) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  assert.ok(service.mail.some((m) => m.to === "email-change@example.com" && /email changed/.test(m.subject)));
});
test("expired sessions and signup tokens fail closed; fresh verification can resume abandoned uncredentialed signup", async () => {
  const expired = "ee".repeat(32);
  await db.query(
    "INSERT INTO dashboard_auth_tokens(hash,email,purpose,expires_at) VALUES ($1,'expired@example.com','signup',1)",
    [await sha256(expired)],
  );
  assert.equal((await call("signup/verify", { token: expired })).status, 400);
  const user = await signup("expired-session@example.com");
  await db.query("UPDATE dashboard_auth_sessions SET idle_until=1 WHERE user_id=$1", [user.p.id]);
  assert.equal((await call("session", {}, user.client, "GET")).status, 401);
  const id = "77".repeat(16);
  await db.query(
    "INSERT INTO dashboard_auth_users(id,email,salt,created_at) VALUES ($1,'abandoned@example.com',$2,1)",
    [id, "88".repeat(16)],
  );
  assert.equal((await call("signup/start", { email: "abandoned@example.com" })).status, 200);
  assert.ok(
    (await db.query("SELECT hash FROM dashboard_auth_tokens WHERE email='abandoned@example.com'"))
      .rows[0],
  );
});
test("an attacker claiming a public wallet cannot block its owner's encrypted signup", async () => {
  const attacker = await signup("wallet-squatter@example.com");
  const victim = await signup("wallet-owner@example.com");
  const publicWallet = "0x" + "ab".repeat(20);
  const attackerKey = Buffer.alloc(32, 91).toString("base64");
  const victimKey = Buffer.alloc(32, 92).toString("base64");
  for (const [user, apiKey, email] of [
    [attacker, attackerKey, "wallet-squatter@example.com"],
    [victim, victimKey, "wallet-owner@example.com"],
  ]) {
    const { envelope } = await operate({ operation: "encrypt", password, parameters: user.p, apiKey, account: publicWallet });
    const response = await call("envelope", { authSecret: user.authSecret, operation: user.operation, envelope }, user.client, "PUT");
    assert.equal(response.status, 200, JSON.stringify(response.data));
    const login = await call("login", { email, authSecret: user.authSecret });
    assert.equal(login.status, 200);
    const decrypted = await operate({ operation: "decrypt", password, parameters: login.data.parameters, envelope: login.data.envelope });
    assert.equal(decrypted.apiKey, apiKey);
  }
  const rows = (await db.query("SELECT id,state FROM dashboard_auth_users WHERE account=$1", [publicWallet])).rows;
  assert.equal(rows.length, 2);
  assert.ok(rows.every((row) => row.state === "active"));
  const theft = await call("envelope", {}, { ...attacker.client, binding: victim.client.binding }, "GET");
  // GET access resolves identity from the authenticated session, never wallet metadata.
  assert.equal(theft.status, 200);
  assert.equal(theft.data.parameters.id, attacker.p.id);
  assert.notEqual(theft.data.parameters.id, victim.p.id);
});
