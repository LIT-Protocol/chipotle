import {
  KDF,
  FORMAT,
  encoder,
  hex,
  unhex,
  randomHex,
  sha256,
  validateEnvelope,
} from "../../lit-static/dapps/dashboard/password-protocol.js";

export interface Env {
  DB: D1Database;
  ENVIRONMENT: string;
  DASHBOARD_URL: string;
  AUTH_SECRET: string;
  RESEND_API_KEY: string;
  MAIL_FROM: string;
}
type User = {
  id: string;
  email: string;
  salt: string;
  verifier: string | null;
  version: number;
  state: string;
  account: string | null;
  envelope: string | null;
  operation: string | null;
};
type Session = {
  hash: string;
  user_id: string;
  version: number;
  csrf: string;
  scope: string;
  expires_at: number;
  idle_until: number;
};
const COOKIE = "__Host-chipotle_auth";
const MINUTE = 60;
const now = () => Math.floor(Date.now() / 1000);
class Failure extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}
const fail = (status: number, message: string): never => {
  throw new Failure(status, message);
};
const json = (value: unknown, status = 200) => Response.json(value, { status });
const email = (v: unknown) => {
  if (
    typeof v !== "string" ||
    v.length > 254 ||
    !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v.trim())
  )
    return fail(400, "Enter a valid email address.");
  return v.trim().toLowerCase();
};
function token(v: unknown): string {
  if (typeof v !== "string" || !/^[0-9a-f]{64}$/.test(v))
    return fail(400, "Invalid request.");
  return v;
}
function parameters(env: Env, u: Pick<User, "id" | "salt" | "version">) {
  return {
    format: FORMAT,
    environment: env.ENVIRONMENT,
    id: u.id,
    salt: u.salt,
    version: u.version,
    kdf: KDF,
  };
}
async function mac(env: Env, value: string) {
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(env.AUTH_SECRET),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  return hex(
    new Uint8Array(
      await crypto.subtle.sign("HMAC", key, encoder.encode(value)),
    ),
  );
}
async function verifier(id: string, secret: unknown) {
  return sha256(
    JSON.stringify(["chipotle-auth-verifier-v1", id, token(secret)]),
  );
}
function equal(a: string, b: string) {
  return crypto.subtle.timingSafeEqual(encoder.encode(a), encoder.encode(b));
}
async function reauthenticate(u: User, secret: unknown) {
  const candidate = await verifier(u.id, secret);
  if (!equal(candidate, u.verifier || "0".repeat(64)))
    fail(401, "Email or password is incorrect.");
}
function cookie(env: Env, value: string, age = 43200) {
  // __Host cookies also work on localhost in modern browsers; dev uses HTTPS if needed.
  return `${COOKIE}=${value}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=${age}`;
}
async function rate(env: Env, label: string, max: number, seconds: number) {
  const t = now();
  const key = await mac(env, `rate:${Math.floor(t / seconds)}:${label}`);
  const row = await env.DB.prepare(
    "INSERT INTO auth_limits(key,count,expires_at) VALUES (?,1,?) ON CONFLICT(key) DO UPDATE SET count=count+1 RETURNING count",
  )
    .bind(key, t + seconds * 2)
    .first<{ count: number }>();
  if (!row || row.count > max)
    fail(429, "Too many attempts. Please try again later.");
}
async function session(request: Request, env: Env, body?: Record<string, any>) {
  const raw = request.headers
    .get("Cookie")
    ?.split(";")
    .map((x) => x.trim())
    .find((x) => x.startsWith(`${COOKIE}=`))
    ?.slice(COOKIE.length + 1);
  if (!raw || !/^[0-9a-f]{64}$/.test(raw))
    return fail(401, "Sign in again to continue.");
  const hash = await sha256(raw);
  const s = await env.DB.prepare(
    "SELECT * FROM auth_sessions WHERE hash=? AND expires_at>? AND idle_until>?",
  )
    .bind(hash, now(), now())
    .first<Session>();
  if (!s) return fail(401, "Sign in again to continue.");
  const u = await env.DB.prepare("SELECT * FROM auth_users WHERE id=?")
    .bind(s.user_id)
    .first<User>();
  if (!u || u.version !== s.version)
    return fail(401, "Sign in again to continue.");
  if (request.method !== "GET") {
    if (request.headers.get("X-CSRF-Token") !== s.csrf)
      fail(403, "Refresh the page and try again.");
    if (body?.id !== u.id || body?.version !== u.version)
      fail(409, "Account changed. Sign in again.");
  }
  await env.DB.prepare("UPDATE auth_sessions SET idle_until=? WHERE hash=?")
    .bind(now() + 30 * MINUTE, hash)
    .run();
  return { s, u };
}
async function issue(env: Env, u: User, scope: string) {
  const raw = randomHex(32),
    hash = await sha256(raw),
    csrf = randomHex(32);
  // Conditional insert closes the password-change/login race.
  await env.DB.prepare(
    "INSERT INTO auth_sessions(hash,user_id,version,csrf,scope,expires_at,idle_until) SELECT ?,id,version,?,?,?,? FROM auth_users WHERE id=? AND version=? AND verifier IS ?",
  )
    .bind(
      hash,
      csrf,
      scope,
      now() + 12 * 3600,
      now() + 30 * MINUTE,
      u.id,
      u.version,
      u.verifier,
    )
    .run();
  const exists = await env.DB.prepare(
    "SELECT hash FROM auth_sessions WHERE hash=?",
  )
    .bind(hash)
    .first();
  if (!exists) fail(409, "Account changed. Sign in again.");
  return { raw, csrf };
}
function userResponse(env: Env, u: User, csrf: string) {
  return {
    email: u.email,
    parameters: parameters(env, u),
    state: u.state,
    operation: u.operation,
    envelope: u.envelope ? JSON.parse(u.envelope) : null,
    csrf,
  };
}
async function mailKey(env: Env) {
  return crypto.subtle.importKey(
    "raw",
    unhex(await mac(env, "outbox-encryption-v1"), 32),
    "AES-GCM",
    false,
    ["encrypt", "decrypt"],
  );
}
async function notification(
  env: Env,
  destination: string,
  subject: string,
  text: string,
) {
  const id = randomHex(16),
    nonce = randomHex(12),
    expiry = now() + 86400;
  const encrypted = await crypto.subtle.encrypt(
    {
      name: "AES-GCM",
      iv: unhex(nonce, 12),
      additionalData: encoder.encode(id),
    },
    await mailKey(env),
    encoder.encode(JSON.stringify({ to: destination, subject, text })),
  );
  return {
    id,
    payload: JSON.stringify({
      nonce,
      ciphertext: hex(new Uint8Array(encrypted)),
    }),
    expiry,
  };
}
async function queueMail(
  env: Env,
  destination: string,
  purpose: "signup" | "email",
  u?: User,
) {
  const raw = randomHex(32),
    hash = await sha256(raw),
    id = randomHex(16),
    nonce = randomHex(12),
    expiry = now() + 30 * MINUTE;
  const url = new URL(env.DASHBOARD_URL);
  url.hash = `verify=${raw}&purpose=${purpose}`;
  const payload = {
    to: destination,
    subject: "Verify your Chipotle email",
    text: `Open this link to verify your email for Chipotle:\n\n${url}\n\nThis link expires in 30 minutes. It cannot reset a password. If you did not request it, ignore this email.`,
  };
  const encrypted = await crypto.subtle.encrypt(
    {
      name: "AES-GCM",
      iv: unhex(nonce, 12),
      additionalData: encoder.encode(id),
    },
    await mailKey(env),
    encoder.encode(JSON.stringify(payload)),
  );
  await env.DB.batch([
    env.DB.prepare(
      "INSERT INTO auth_tokens(hash,email,user_id,version,purpose,expires_at) VALUES (?,?,?,?,?,?)",
    ).bind(
      hash,
      destination,
      u?.id || null,
      u?.version || null,
      purpose,
      expiry,
    ),
    env.DB.prepare(
      "INSERT INTO auth_outbox(id,payload,next_at,expires_at) VALUES (?,?,?,?)",
    ).bind(
      id,
      JSON.stringify({ nonce, ciphertext: hex(new Uint8Array(encrypted)) }),
      now(),
      expiry,
    ),
  ]);
}
export async function deliverMail(env: Env) {
  const rows = await env.DB.prepare(
    "SELECT * FROM auth_outbox WHERE next_at<=? AND expires_at>? AND attempts<6 LIMIT 20",
  )
    .bind(now(), now())
    .all<{ id: string; payload: string; attempts: number }>();
  for (const row of rows.results) {
    const claim = await env.DB.prepare(
      "UPDATE auth_outbox SET attempts=attempts+1,next_at=? WHERE id=? AND attempts=?",
    )
      .bind(now() + 300, row.id, row.attempts)
      .run();
    if (!claim.meta.changes) continue;
    try {
      const { nonce, ciphertext } = JSON.parse(row.payload);
      const plain = await crypto.subtle.decrypt(
        {
          name: "AES-GCM",
          iv: unhex(nonce, 12),
          additionalData: encoder.encode(row.id),
        },
        await mailKey(env),
        unhex(ciphertext, ciphertext.length / 2),
      );
      const payload = JSON.parse(new TextDecoder().decode(plain));
      const result = await fetch("https://api.resend.com/emails", {
        signal: AbortSignal.timeout(15000),
        method: "POST",
        headers: {
          Authorization: `Bearer ${env.RESEND_API_KEY}`,
          "Content-Type": "application/json",
          "Idempotency-Key": row.id,
        },
        body: JSON.stringify({ from: env.MAIL_FROM, ...payload }),
      });
      if (result.ok)
        await env.DB.prepare("DELETE FROM auth_outbox WHERE id=?")
          .bind(row.id)
          .run();
      else console.error("auth_mail_delivery_failed", result.status);
    } catch {
      console.error("auth_mail_delivery_failed");
    }
  }
}
async function routes(request: Request, env: Env, ctx: ExecutionContext) {
  const path = new URL(request.url).pathname;
  if (path === "/health" && request.method === "GET") return json({ ok: true });
  if (
    !env.AUTH_SECRET ||
    env.AUTH_SECRET.length < 32 ||
    !/^[a-z0-9-]{1,40}$/.test(env.ENVIRONMENT)
  )
    return fail(503, "Account service is not configured.");
  const origin = new URL(env.DASHBOARD_URL).origin;
  if (request.headers.get("Origin") !== origin)
    return fail(403, "Origin not allowed.");
  if (request.method === "OPTIONS") return new Response(null, { status: 204 });
  if (request.headers.get("X-Chipotle-Auth") !== "1")
    return fail(403, "Invalid client request.");
  await rate(
    env,
    `ip:${request.headers.get("CF-Connecting-IP") || "unknown"}`,
    120,
    600,
  );
  let b: Record<string, any> = {};
  if (request.method !== "GET") {
    if (!request.headers.get("Content-Type")?.startsWith("application/json"))
      return fail(415, "Use JSON.");
    if (Number(request.headers.get("Content-Length") || 0) > 8192)
      return fail(413, "Request too large.");
    // Bound streamed bodies too; Content-Length is not an authorization boundary.
    const reader = request.body?.getReader();
    let size = 0;
    const chunks: Uint8Array[] = [];
    if (reader)
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        size += value.byteLength;
        if (size > 8192) {
          await reader.cancel();
          return fail(413, "Request too large.");
        }
        chunks.push(value);
      }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.length;
    }
    try {
      b = JSON.parse(new TextDecoder().decode(bytes));
    } catch {
      return fail(400, "Invalid JSON.");
    }
    if (!b || typeof b !== "object" || Array.isArray(b))
      return fail(400, "Invalid JSON.");
  }
  if (path === "/auth/v1/signup/start" && request.method === "POST") {
    const address = email(b.email);
    await rate(env, `signup:${address}`, 3, 3600);
    if (!env.RESEND_API_KEY)
      return fail(503, "Email verification is unavailable.");
    const existing = await env.DB.prepare(
      "SELECT state,verifier FROM auth_users WHERE email=?",
    )
      .bind(address)
      .first<User>();
    if (!existing || (existing.state === "verified" && !existing.verifier)) {
      await queueMail(env, address, "signup");
      ctx.waitUntil(deliverMail(env));
    }
    return json({
      message:
        "If this email can create an account, a verification link is on its way. Existing users should sign in.",
    });
  }
  if (path === "/auth/v1/signup/verify" && request.method === "POST") {
    const hash = await sha256(token(b.token));
    const raw = randomHex(32),
      claim = await sha256(raw),
      csrf = randomHex(32),
      id = randomHex(16),
      salt = randomHex(16);
    await env.DB.batch([
      env.DB.prepare(
        "UPDATE auth_tokens SET claim=? WHERE hash=? AND purpose='signup' AND claim IS NULL AND expires_at>?",
      ).bind(claim, hash, now()),
      env.DB.prepare(
        "INSERT OR IGNORE INTO auth_users(id,email,salt,created_at) SELECT ?,email,?,? FROM auth_tokens WHERE hash=? AND claim=?",
      ).bind(id, salt, now(), hash, claim),
      env.DB.prepare(
        "INSERT INTO auth_sessions(hash,user_id,version,csrf,scope,expires_at,idle_until) SELECT ?,u.id,u.version,?,'signup',?,? FROM auth_users u JOIN auth_tokens t ON t.email=u.email WHERE t.hash=? AND t.claim=? AND u.state='verified' AND u.verifier IS NULL",
      ).bind(claim, csrf, now() + 3600, now() + 1800, hash, claim),
    ]);
    const u = await env.DB.prepare(
      "SELECT u.* FROM auth_users u JOIN auth_sessions s ON s.user_id=u.id WHERE s.hash=?",
    )
      .bind(claim)
      .first<User>();
    if (!u)
      return fail(
        400,
        "This verification link is expired or already used. Sign in or request a new link.",
      );
    const response = json(userResponse(env, u, csrf));
    response.headers.set("Set-Cookie", cookie(env, raw, 3600));
    return response;
  }
  if (path === "/auth/v1/login/parameters" && request.method === "POST") {
    const address = email(b.email);
    await rate(env, `parameters:${address}`, 30, 600);
    const u = await env.DB.prepare(
      "SELECT * FROM auth_users WHERE email=? AND verifier IS NOT NULL",
    )
      .bind(address)
      .first<User>();
    const fake = {
      id: (await mac(env, `identity:${address}`)).slice(0, 32),
      salt: (await mac(env, `salt:${address}`)).slice(0, 32),
      version: 1,
    };
    return json(parameters(env, u || fake));
  }
  if (path === "/auth/v1/login" && request.method === "POST") {
    const address = email(b.email);
    await rate(env, `login:${address}`, 10, 600);
    const u = await env.DB.prepare("SELECT * FROM auth_users WHERE email=?")
      .bind(address)
      .first<User>();
    const candidate = await verifier(
      u?.id || (await mac(env, `identity:${address}`)).slice(0, 32),
      b.authSecret,
    );
    if (!equal(candidate, u?.verifier || "0".repeat(64)) || !u?.verifier)
      return fail(401, "Email or password is incorrect.");
    const issued = await issue(
      env,
      u,
      u.state === "active" ? "account" : "signup",
    );
    const response = json(userResponse(env, u, issued.csrf));
    response.headers.set("Set-Cookie", cookie(env, issued.raw));
    return response;
  }
  // Reject absent/nonexistent APIs, including reset/import, before touching sessions.
  const allowed = new Set([
    "GET /auth/v1/session",
    "POST /auth/v1/logout",
    "POST /auth/v1/logout-all",
    "POST /auth/v1/signup/credentials",
    "POST /auth/v1/signup/begin",
    "PUT /auth/v1/envelope",
    "GET /auth/v1/envelope",
    "POST /auth/v1/password/change",
    "POST /auth/v1/email/start",
    "POST /auth/v1/email/complete",
  ]);
  if (!allowed.has(`${request.method} ${path}`)) return fail(404, "Not found.");
  const { s, u } = await session(request, env, b);
  if (
    path === "/auth/v1/session" ||
    (path === "/auth/v1/envelope" && request.method === "GET")
  )
    return json(userResponse(env, u, s.csrf));
  if (path === "/auth/v1/logout" || path === "/auth/v1/logout-all") {
    await env.DB.prepare(
      path.endsWith("logout-all")
        ? "DELETE FROM auth_sessions WHERE user_id=?"
        : "DELETE FROM auth_sessions WHERE hash=?",
    )
      .bind(path.endsWith("logout-all") ? u.id : s.hash)
      .run();
    const response = json({ ok: true });
    response.headers.set("Set-Cookie", cookie(env, "", 0));
    return response;
  }
  if (path === "/auth/v1/signup/credentials") {
    if (s.scope !== "signup") return fail(403, "Signup session required.");
    const value = await verifier(u.id, b.authSecret);
    const result = await env.DB.prepare(
      "UPDATE auth_users SET verifier=?,state='reserved',operation=? WHERE id=? AND state='verified' AND verifier IS NULL AND version=?",
    )
      .bind(value, randomHex(16), u.id, u.version)
      .run();
    if (!result.meta.changes)
      return fail(409, "Signup has already started. Sign in to continue.");
    return json({ ok: true });
  }
  if (path === "/auth/v1/signup/begin") {
    if (s.scope !== "signup") return fail(403, "Signup session required.");
    await reauthenticate(u, b.authSecret);
    const result = await env.DB.prepare(
      "UPDATE auth_users SET state='creating' WHERE id=? AND state='reserved' AND version=? AND verifier=?",
    )
      .bind(u.id, u.version, u.verifier)
      .run();
    if (!result.meta.changes)
      return fail(
        409,
        "Account creation already started. Do not create another account; resume saving the original key.",
      );
    return json({ operation: u.operation });
  }
  if (path === "/auth/v1/envelope" && request.method === "PUT") {
    if (s.scope !== "signup") return fail(403, "Signup session required.");
    await reauthenticate(u, b.authSecret);
    try {
      validateEnvelope(b.envelope, parameters(env, u));
    } catch {
      return fail(400, "Invalid encrypted account record.");
    }
    if (b.operation !== u.operation)
      return fail(409, "Wrong signup operation.");
    const encoded = JSON.stringify(b.envelope);
    if (u.state === "active") {
      if (u.envelope !== encoded) return fail(409, "Account already created.");
      return json({ ok: true });
    }
    const result = await env.DB.prepare(
      "UPDATE auth_users SET account=?,envelope=?,state='active' WHERE id=? AND state='creating' AND version=? AND verifier=?",
    )
      .bind(b.envelope.account, encoded, u.id, u.version, u.verifier)
      .run();
    if (!result.meta.changes)
      return fail(409, "Account changed. Sign in again.");
    return json({ ok: true });
  }
  if (u.state !== "active" || s.scope !== "account")
    return fail(403, "Sign in to your completed account first.");
  if (path === "/auth/v1/password/change") {
    await reauthenticate(u, b.oldAuthSecret);
    token(b.authSecret);
    const p = {
      ...parameters(env, u),
      version: u.version + 1,
      salt: b.envelope?.salt,
    };
    try {
      validateEnvelope(b.envelope, p);
    } catch {
      return fail(400, "Invalid encrypted account record.");
    }
    if (
      b.envelope.account !== u.account ||
      b.envelope.salt === u.salt ||
      b.envelope.nonce === JSON.parse(u.envelope!).nonce
    )
      return fail(
        400,
        "Password changes require fresh encryption parameters and the same account.",
      );
    const newVerifier = await verifier(u.id, b.authSecret);
    const notice = await notification(
      env,
      u.email,
      "Chipotle password changed",
      "Your Chipotle password was changed. Existing exported API keys remain valid. If this was not you, review your account access.",
    );
    const results = await env.DB.batch([
      env.DB.prepare(
        "UPDATE auth_users SET verifier=?,salt=?,envelope=?,version=version+1 WHERE id=? AND version=? AND verifier=?",
      ).bind(
        newVerifier,
        p.salt,
        JSON.stringify(b.envelope),
        u.id,
        u.version,
        u.verifier,
      ),
      env.DB.prepare(
        "INSERT INTO auth_outbox(id,payload,next_at,expires_at) SELECT ?,?,?,? WHERE changes()=1",
      ).bind(notice.id, notice.payload, now(), notice.expiry),
    ]);
    if (!results[0].meta.changes)
      return fail(409, "Account changed. Sign in again.");
    ctx.waitUntil(deliverMail(env));
    // Version bump already invalidates sessions atomically; cleanup cannot revoke newer sessions.
    await env.DB.batch([
      env.DB.prepare(
        "DELETE FROM auth_sessions WHERE user_id=? AND version<=?",
      ).bind(u.id, u.version),
      env.DB.prepare(
        "DELETE FROM auth_tokens WHERE user_id=? AND version<=?",
      ).bind(u.id, u.version),
    ]);
    const response = json({ ok: true });
    response.headers.set("Set-Cookie", cookie(env, "", 0));
    return response;
  }
  if (path === "/auth/v1/email/start") {
    await reauthenticate(u, b.authSecret);
    const address = email(b.email);
    await rate(env, `email:${u.id}`, 3, 3600);
    if (!env.RESEND_API_KEY)
      return fail(503, "Email verification is unavailable.");
    if (
      !(await env.DB.prepare("SELECT id FROM auth_users WHERE email=?")
        .bind(address)
        .first())
    ) {
      await queueMail(env, address, "email", u);
      ctx.waitUntil(deliverMail(env));
    }
    return json({
      message:
        "If this address is available, a verification link is on its way.",
    });
  }
  if (path === "/auth/v1/email/complete") {
    await reauthenticate(u, b.authSecret);
    const hash = await sha256(token(b.token)),
      claim = randomHex(32);
    // Fail early with a clear status when the verified address was registered by
    // someone else after the link was sent; the transactional batch below still
    // guards the race (its UNIQUE violation surfaces as a generic failure).
    const taken = await env.DB.prepare(
      "SELECT 1 FROM auth_users WHERE id!=? AND email=(SELECT email FROM auth_tokens WHERE hash=? AND user_id=? AND purpose='email')",
    )
      .bind(u.id, hash, u.id)
      .first();
    if (taken)
      return fail(
        409,
        "That email address is already in use by another account.",
      );
    const notice = await notification(
      env,
      u.email,
      "Chipotle email changed",
      "Your Chipotle sign-in email was changed. Your password and API key have not changed. If this was not you, review your account access.",
    );
    // Incrementing the version would invalidate the ciphertext AAD. Email is deliberately
    // outside that AAD: delete service sessions and old email proofs in this transaction.
    await env.DB.batch([
      env.DB.prepare(
        "UPDATE auth_tokens SET claim=? WHERE hash=? AND user_id=? AND version=? AND purpose='email' AND claim IS NULL AND expires_at>? AND EXISTS(SELECT 1 FROM auth_users WHERE id=? AND version=? AND verifier=?)",
      ).bind(claim, hash, u.id, u.version, now(), u.id, u.version, u.verifier),
      env.DB.prepare(
        "UPDATE auth_users SET email=(SELECT email FROM auth_tokens WHERE hash=? AND claim=?) WHERE id=? AND EXISTS(SELECT 1 FROM auth_tokens WHERE hash=? AND claim=?)",
      ).bind(hash, claim, u.id, hash, claim),
      env.DB.prepare(
        "INSERT INTO auth_outbox(id,payload,next_at,expires_at) SELECT ?,?,?,? WHERE EXISTS(SELECT 1 FROM auth_tokens WHERE hash=? AND claim=?)",
      ).bind(notice.id, notice.payload, now(), notice.expiry, hash, claim),
      env.DB.prepare(
        "DELETE FROM auth_sessions WHERE user_id=? AND EXISTS(SELECT 1 FROM auth_tokens WHERE hash=? AND claim=?)",
      ).bind(u.id, hash, claim),
      env.DB.prepare(
        "DELETE FROM auth_tokens WHERE user_id=? AND hash!=? AND EXISTS(SELECT 1 FROM auth_tokens WHERE hash=? AND claim=?)",
      ).bind(u.id, hash, hash, claim),
    ]);
    const used = await env.DB.prepare(
      "SELECT hash FROM auth_tokens WHERE hash=? AND claim=?",
    )
      .bind(hash, claim)
      .first();
    if (!used)
      return fail(400, "Email verification link is expired or already used.");
    ctx.waitUntil(deliverMail(env));
    const response = json({ ok: true });
    response.headers.set("Set-Cookie", cookie(env, "", 0));
    return response;
  }
  return fail(404, "Not found.");
}
export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext) {
    let response: Response;
    try {
      response = await routes(request, env, ctx);
    } catch (e) {
      if (e instanceof Failure) response = json({ error: e.message }, e.status);
      else {
        console.error("auth_request_failed");
        response = json(
          { error: "Account service unavailable. Please try again." },
          503,
        );
      }
    }
    response.headers.set("Cache-Control", "no-store");
    response.headers.set("X-Content-Type-Options", "nosniff");
    if (
      env.DASHBOARD_URL &&
      request.headers.get("Origin") === new URL(env.DASHBOARD_URL).origin
    ) {
      response.headers.set(
        "Access-Control-Allow-Origin",
        new URL(env.DASHBOARD_URL).origin,
      );
      response.headers.set("Vary", "Origin");
      response.headers.set("Access-Control-Allow-Credentials", "true");
      response.headers.set(
        "Access-Control-Allow-Methods",
        "GET, POST, PUT, OPTIONS",
      );
      response.headers.set(
        "Access-Control-Allow-Headers",
        "Content-Type, X-Chipotle-Auth, X-CSRF-Token",
      );
    }
    return response;
  },
  async scheduled(
    _event: ScheduledController,
    env: Env,
    ctx: ExecutionContext,
  ) {
    ctx.waitUntil(
      (async () => {
        await deliverMail(env);
        await env.DB.batch([
          env.DB.prepare("DELETE FROM auth_tokens WHERE expires_at<?").bind(
            now(),
          ),
          env.DB.prepare(
            "DELETE FROM auth_sessions WHERE expires_at<? OR idle_until<?",
          ).bind(now(), now()),
          env.DB.prepare("DELETE FROM auth_outbox WHERE expires_at<?").bind(
            now(),
          ),
          env.DB.prepare("DELETE FROM auth_limits WHERE expires_at<?").bind(
            now(),
          ),
        ]);
      })(),
    );
  },
};
