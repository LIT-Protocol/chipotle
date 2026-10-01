import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { privateKeyToAccount } from "viem/accounts";
import {
  OwnerClient,
  LitConnection,
  authorizationTypedData,
  type Authority,
} from "../sdk/src/index.ts";
import { randomBytes, hex } from "../protocol/crypto.ts";
import { HttpError } from "../protocol/client-http.ts";
import { authenticatorCode } from "./totp-fixture.ts";

const api = process.env.KEYCHAIN_TEST_API;
const lit = process.env.KEYCHAIN_TEST_LIT || "http://127.0.0.1:55440";
const database = process.env.KEYCHAIN_TEST_DATABASE_URL;
const post = (body: unknown): RequestInit => ({
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(body),
});

test(
  "2FA: verified enrollment, login enforcement, replay, recovery, management approval, expiration and throttling",
  { skip: !api || !database },
  async () => {
    assert.ok(["localhost", "127.0.0.1"].includes(new URL(api!).hostname));
    assert.ok(["localhost", "127.0.0.1"].includes(new URL(database!).hostname));
    assert.match(new URL(database!).pathname, /test|_ci$/);
    const sql = (query: string) =>
      execFileSync(
        "psql",
        [database!, "-At", "-v", "ON_ERROR_STOP=1", "-c", query],
        { encoding: "utf8" },
      ).trim();
    const original = globalThis.fetch;
    let cookie = "";
    globalThis.fetch = async (input, init) => {
      if (!String(input).startsWith(api!)) return original(input, init);
      const headers = new Headers(init?.headers);
      if (cookie) headers.set("Cookie", cookie);
      const res = await original(input, { ...init, headers });
      const set = res.headers.get("set-cookie");
      if (set) cookie = set.split(";")[0];
      return res;
    };
    try {
      const account = privateKeyToAccount(`0x${hex(randomBytes())}`);
      const owner = {
        kind: "wallet" as const,
        address: account.address.toLowerCase(),
      };
      const authority: Authority = {
        v: 2,
        network: "test",
        registry: api!,
        owner,
      };
      const c = new OwnerClient(
        authority,
        async (challenge) => ({
          kind: "wallet",
          owner,
          challenge,
          signature: await account.signTypedData(
            authorizationTypedData(challenge),
          ),
        }),
        new LitConnection(lit),
      );
      await c.login();
      const firstCookie = cookie;
      await c.login(); // A second pre-enrollment browser session.
      assert.equal((await c.securityStatus()).enabled, false);
      const approval = async (operation: string) => {
        const { authorityCid, ...doc } = await c.api(
          "/api/security/challenge",
          post({ operation }),
        );
        return c.authorize(doc, authorityCid);
      };
      const setupApproval = await approval("setup");
      // A security approval cannot be exchanged for a normal login session.
      await assert.rejects(
        c.api("/auth/login", post({ authority, authorization: setupApproval })),
        /challenge_used_or_expired/,
      );
      assert.equal(
        (
          await original(`${api}/api/security/totp/setup`, {
            ...post(setupApproval),
            headers: {
              "Content-Type": "application/json",
              Cookie: firstCookie,
            },
          })
        ).status,
        403,
      );
      const setup = await c.api(
        "/api/security/totp/setup",
        post(setupApproval),
      );
      await assert.rejects(
        c.api("/api/security/totp/setup", post(setupApproval)),
        /approval_used_or_expired/,
      );
      assert.match(setup.secret, /^[A-Z2-7]{32}$/);
      assert.equal(new URL(setup.uri).searchParams.get("secret"), setup.secret);
      assert.equal((await c.securityStatus()).enabled, false);
      const encrypted = sql(
        `SELECT encrypted_secret FROM kc_two_factor_setup WHERE vault_id='${c.vaultId}'`,
      );
      assert.ok(!encrypted.includes(setup.secret));
      await assert.rejects(c.confirmTwoFactor("bad"), /invalid_or_used/);
      const step = Math.floor(Date.now() / 30000);
      const enrolledCode = authenticatorCode(setup.secret, step);
      assert.equal(
        (
          await original(`${api}/api/security/totp/confirm`, {
            ...post({ code: enrolledCode }),
            headers: {
              "Content-Type": "application/json",
              Cookie: firstCookie,
            },
          })
        ).status,
        410,
      );
      assert.equal(
        (
          await fetch(`${api}/api/security/totp/confirm`, {
            ...post({ code: enrolledCode }),
            headers: {
              "Content-Type": "application/json",
              Origin: "https://attacker.example",
            },
          })
        ).status,
        403,
      );
      const { recoveryCodes } = await c.confirmTwoFactor(enrolledCode);
      assert.equal(recoveryCodes.length, 10);
      assert.equal(new Set(recoveryCodes).size, 10);
      assert.deepEqual(await c.securityStatus(), {
        enabled: true,
        recoveryCodesRemaining: 10,
      });
      assert.equal(
        (await original(`${api}/api/me`, { headers: { Cookie: firstCookie } }))
          .status,
        401,
      );
      assert.ok(
        !sql(
          `SELECT code_hash FROM kc_two_factor_recovery WHERE vault_id='${c.vaultId}'`,
        ).includes(recoveryCodes[0]),
      );
      const activeCookie = cookie;
      const proof = await approval("disable");
      await assert.rejects(
        c.api(
          "/api/security/totp/regenerate",
          post({ authorization: proof, code: recoveryCodes[0] }),
        ),
        /approval_used_or_expired/,
      );
      await assert.rejects(
        c.api(
          "/api/security/totp/disable",
          post({ authorization: proof, code: "bad" }),
        ),
        /invalid_or_used/,
      );
      // A session plus a code cannot change settings without fresh owner approval.
      assert.equal(
        (
          await fetch(
            `${api}/api/security/totp/disable`,
            post({ code: recoveryCodes[0] }),
          )
        ).status,
        422,
      );
      cookie = "";
      await assert.rejects(c.login(), /Two-factor authentication required/);
      assert.equal(cookie, "");
      assert.equal((await original(`${api}/api/me`)).status, 401);
      assert.equal(
        (await original(`${api}/api/execution-key`, post({}))).status,
        401,
      );
      c.secondFactor = async (verify) => {
        assert.equal(cookie, "");
        await assert.rejects(verify(enrolledCode), /invalid_or_used/); // Enrollment consumed this time step.
        await verify(recoveryCodes[0]);
      };
      await c.login();
      assert.equal((await c.securityStatus()).recoveryCodesRemaining, 9);
      cookie = "";
      c.secondFactor = async (verify) => {
        await assert.rejects(verify(recoveryCodes[0]), /invalid_or_used/);
        await verify(recoveryCodes[1]);
      };
      await c.login();
      // Reset only this fixture's rate bucket to exercise additional scenarios.
      sql(`DELETE FROM kc_budgets WHERE bucket='totp:${c.vaultId}'`);
      const pendingLogin = async () => {
        const { authorityCid, ...doc } = await c.api(
          "/auth/challenge",
          post(authority),
        );
        const authorization = await c.authorize(doc, authorityCid || undefined);
        const result = await c.api(
          "/auth/login",
          post({ authority, authorization }),
        );
        assert.equal(result.twoFactorRequired, true);
        return result.token as string;
      };
      const pending = await pendingLogin();
      const next = authenticatorCode(
        setup.secret,
        Math.max(step + 1, Math.floor(Date.now() / 30000)),
      );
      const complete = (token: string, code: string) =>
        c.api("/auth/two-factor", post({ token, code }));
      await complete(pending, next);
      await assert.rejects(
        complete(pending, recoveryCodes[2]),
        /two_factor_login_expired/,
      );
      const pendingA = await pendingLogin();
      const pendingB = await pendingLogin();
      const races = await Promise.allSettled([
        complete(pendingA, recoveryCodes[2]),
        complete(pendingB, recoveryCodes[2]),
      ]);
      assert.equal(races.filter((r) => r.status === "fulfilled").length, 1);
      assert.equal((await c.securityStatus()).recoveryCodesRemaining, 7);
      const expired = await pendingLogin();
      sql(
        `UPDATE kc_two_factor_logins SET expires_at=now()-interval '1 second' WHERE vault_id='${c.vaultId}'`,
      );
      await assert.rejects(
        complete(expired, recoveryCodes[3]),
        /two_factor_login_expired/,
      );
      const priorLogin = await pendingLogin();
      const replacement = await c.regenerateRecoveryCodes(recoveryCodes[3]);
      await assert.rejects(
        complete(priorLogin, recoveryCodes[4]),
        /two_factor_login_expired/,
      );
      assert.equal(
        (await original(`${api}/api/me`, { headers: { Cookie: activeCookie } }))
          .status,
        401,
      );
      assert.equal((await c.securityStatus()).recoveryCodesRemaining, 10);
      await assert.rejects(
        c.disableTwoFactor(recoveryCodes[4]),
        /invalid_or_used/,
      );
      await c.disableTwoFactor(replacement.recoveryCodes[0]);
      assert.deepEqual(await c.securityStatus(), {
        enabled: false,
        recoveryCodesRemaining: 0,
      });
      c.secondFactor = async () => {
        throw new Error("2FA should be off");
      };
      cookie = "";
      await c.login();
      const events = (await c.api("/api/audit")).events.map(
        (e: any) => e.event,
      );
      for (const event of [
        "two_factor_enabled",
        "two_factor_disabled",
        "two_factor_recovery_used",
        "two_factor_recovery_regenerated",
      ])
        assert.ok(events.includes(event));
      await c.setupTwoFactor();
      sql(
        `UPDATE kc_two_factor_setup SET expires_at=now()-interval '1 second' WHERE vault_id='${c.vaultId}'`,
      );
      await assert.rejects(
        c.confirmTwoFactor("123456"),
        /two_factor_setup_expired/,
      );
      const latest = await c.setupTwoFactor();
      sql(`DELETE FROM kc_budgets WHERE bucket='totp:${c.vaultId}'`);
      for (let n = 0; n < 10; n++)
        await assert.rejects(c.confirmTwoFactor("bad"), /invalid_or_used/);
      await assert.rejects(
        c.confirmTwoFactor(authenticatorCode(latest.secret)),
        (e: unknown) => e instanceof HttpError && e.status === 429,
      );
      assert.equal((await c.securityStatus()).enabled, false);
      sql(`DELETE FROM kc_budgets WHERE bucket='totp:${c.vaultId}'`);
      const finalCodes = (
        await c.confirmTwoFactor(authenticatorCode(latest.secret))
      ).recoveryCodes;
      const beforeCredentialsChange = await pendingLogin();
      const recoveryAccount = privateKeyToAccount(`0x${hex(randomBytes())}`);
      const recoveryOwner = {
        kind: "wallet" as const,
        address: recoveryAccount.address.toLowerCase(),
      };
      await c.updateCredentials([owner, recoveryOwner]);
      await assert.rejects(
        complete(beforeCredentialsChange, finalCodes[0]),
        /two_factor_login_expired/,
      );
      const alternate = new OwnerClient(
        authority,
        async (challenge) => ({
          kind: "wallet",
          owner: recoveryOwner,
          challenge,
          signature: await recoveryAccount.signTypedData(
            authorizationTypedData(challenge),
          ),
        }),
        new LitConnection(lit),
      );
      await assert.rejects(
        alternate.login(),
        /Two-factor authentication required/,
      );
      alternate.secondFactor = (verify) => verify(finalCodes[0]);
      await alternate.login();
      assert.equal((await alternate.securityStatus()).enabled, true);
    } finally {
      globalThis.fetch = original;
    }
  },
);
