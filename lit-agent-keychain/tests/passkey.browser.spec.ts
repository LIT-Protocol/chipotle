import { test, expect } from "@playwright/test";
import { p256 } from "@noble/curves/nist.js";
import { sha256 } from "@noble/hashes/sha2.js";
import {
  b64u,
  unb64u,
  digest,
  unhex,
  randomId,
  nowSeconds,
} from "../protocol/crypto.ts";
import type { Challenge } from "../protocol/schema.ts";

// Isolated browser context, no production credentials or services.
test.beforeEach(async ({ page }) => {
  // Surface the actual script URL/status instead of only Chromium's generic
  // "Failed to fetch dynamically imported module". Never log response bodies.
  page.on("response", (response) => {
    if (response.request().resourceType() === "script" && !response.ok())
      console.error(`Script HTTP ${response.status()}: ${response.url()}`);
  });
  page.on("requestfailed", (request) => {
    if (request.resourceType() === "script")
      console.error(
        `Script request failed: ${request.url()} (${request.failure()?.errorText})`,
      );
  });
  await page.route("**/identity-test", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: "<!doctype html><title>Passkey regression</title>",
    }),
  );
  await page.goto("/identity-test");
});

for (const scenario of [
  { hint: "original", selected: "recovery" },
  { hint: "recovery", selected: "original" },
  { hint: "recovery", selected: "recovery" },
] as const) {
  test(`cached ${scenario.hint}, available ${scenario.selected}: discover the existing vault`, async ({
    page,
    context,
  }) => {
    const cdp = await context.newCDPSession(page);
    await cdp.send("WebAuthn.enable");
    const { authenticatorId } = await cdp.send(
      "WebAuthn.addVirtualAuthenticator",
      {
        options: {
          protocol: "ctap2",
          transport: "internal",
          hasResidentKey: true,
          hasUserVerification: true,
          isUserVerified: true,
          automaticPresenceSimulation: true,
        },
      },
    );
    const owners = await page.evaluate(async (hint) => {
      const { createPasskey } = await import(
        /* @vite-ignore */ String("/identities.js")
      );
      const original = (await createPasskey("Original")).owner;
      const recovery = (await createPasskey("Recovery")).owner;
      // Reproduce the production stale hint, without persisting any private key.
      localStorage.setItem(
        "keychain.passkey",
        JSON.stringify(hint === "original" ? original : recovery),
      );
      return { original, recovery };
    }, scenario.hint);
    if (
      owners.original.kind !== "passkey" ||
      owners.recovery.kind !== "passkey"
    )
      throw new Error("Expected passkeys");
    const selected = owners[scenario.selected];
    const removed =
      owners[scenario.selected === "original" ? "recovery" : "original"];
    await cdp.send("WebAuthn.removeCredential", {
      authenticatorId,
      credentialId: Buffer.from(unb64u(removed.credentialId)).toString(
        "base64",
      ),
    });
    const authority = {
      v: 2,
      network: "test",
      registry: "http://localhost:55449",
      owner: owners.original,
    };
    let lookups = 0;
    await page.route(`**/api/passkeys/${selected.credentialId}`, (route) => {
      lookups++;
      return route.fulfill({ json: { owner: selected, authority } });
    });
    const found = await page.evaluate(async () => {
      const { discoverPasskey, ownerClient } = await import(
        /* @vite-ignore */ String("/identities.js")
      );
      const found = await discoverPasskey();
      (window as any).recoveryIdentity = found.identity;
      return {
        owner: found.identity.owner,
        authority: ownerClient(found.identity, "test", found.authority)
          .authority,
      };
    });
    expect(found.owner).toEqual(selected);
    expect(found.authority).toEqual(authority);
    expect(lookups).toBe(1);
    const now = nowSeconds();
    const challenge: Challenge = {
      v: 2,
      domain: "lit-keychain/authorize/v2",
      vaultId: digest(authority),
      objectHash: randomId(),
      operation: "login",
      nonce: randomId(),
      issuedAt: now,
      expiresAt: now + 120,
    };
    const proof = await page.evaluate(
      (challenge) => (window as any).recoveryIdentity.signer(challenge),
      challenge,
    );
    expect(proof.owner).toEqual(selected);
    const clientData = unb64u(proof.clientDataJSON);
    expect(JSON.parse(new TextDecoder().decode(clientData)).challenge).toBe(
      b64u(unhex(digest(challenge))),
    );
    const authData = unb64u(proof.authenticatorData);
    expect(authData[32] & 5).toBe(5); // Presence and verification remain required.
    const signed = new Uint8Array([...authData, ...sha256(clientData)]);
    expect(
      p256.verify(unb64u(proof.signature), signed, unhex(selected.publicKey), {
        format: "der",
        lowS: false,
      }),
    ).toBe(true);
  });
}

// Real UI/WebAuthn, mocked public network; stop before backend/TEE login.
for (const scenario of [
  "duplicate root vault",
  "missing backend account",
  "unknown selected credential",
] as const) {
  test(`explicit backup: ${scenario}`, async ({ page, context }) => {
    const cdp = await context.newCDPSession(page);
    await cdp.send("WebAuthn.enable");
    const { authenticatorId } = await cdp.send(
      "WebAuthn.addVirtualAuthenticator",
      {
        options: {
          protocol: "ctap2",
          transport: "internal",
          hasResidentKey: true,
          hasUserVerification: true,
          isUserVerified: true,
          automaticPresenceSimulation: true,
        },
      },
    );
    const owners = await page.evaluate(async () => {
      const { createPasskey } = await import(
        /* @vite-ignore */ String("/identities.js")
      );
      return {
        original: (await createPasskey("Original")).owner,
        recovery: (await createPasskey("Recovery")).owner,
      };
    });
    await cdp.send("WebAuthn.removeCredential", {
      authenticatorId,
      credentialId: Buffer.from(
        unb64u(
          owners[
            scenario === "missing backend account" ? "recovery" : "original"
          ].credentialId,
        ),
      ).toString("base64"),
    });
    const authority = {
      v: 2,
      network: "test",
      registry: "http://localhost:55449",
      owner: owners.original,
    };
    await page.route("**/api/config", (route) =>
      route.fulfill({ json: { network: "test" } }),
    );
    await page.route("**/api/passkeys/*", (route) =>
      scenario !== "duplicate root vault"
        ? route.fulfill({
            status: 404,
            json: { error: "passkey_not_registered" },
          })
        : route.fulfill({
            json: {
              owner: owners.recovery,
              authority: { ...authority, owner: owners.recovery },
            },
          }),
    );
    let requested: unknown;
    await page.route("**/auth/challenge", (route) => {
      requested = route.request().postDataJSON();
      return route.fulfill({ status: 503, json: { error: "test_boundary" } });
    });
    await page.goto("/");
    await page.getByText("Recover an existing vault", { exact: true }).click();
    await page.getByLabel("Backup file").setInputFiles({
      name: "synthetic-backup.json",
      mimeType: "application/json",
      buffer: Buffer.from(
        JSON.stringify({ v: 2, authority, credentials: null, bundles: [] }),
      ),
    });
    await expect(
      page.getByText("Backup loaded.", { exact: false }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Use an existing passkey", exact: true })
      .click();
    if (scenario === "unknown selected credential") {
      await expect(page.getByRole("alert")).toContainText(
        "passkey_not_registered",
      );
      expect(requested).toBeUndefined();
    } else {
      await expect.poll(() => requested).toEqual(authority);
    }
  });
}

for (const operation of ["discover", "create", "approve"] as const) {
  test(`${operation}: cancellation gives retry and recovery guidance without auto-retrying`, async ({
    page,
  }) => {
    const result = await page.evaluate(async (operation) => {
      const identities = await import(
        /* @vite-ignore */ String("/identities.js")
      );
      let calls = 0;
      const cancel = async () => {
        calls++;
        throw new DOMException("Raw browser error", "NotAllowedError");
      };
      navigator.credentials.get = cancel;
      navigator.credentials.create = cancel;
      try {
        if (operation === "discover") await identities.discoverPasskey();
        else if (operation === "create") await identities.createPasskey("Test");
        else
          await identities
            .passkeyIdentity({
              kind: "passkey",
              credentialId: "AA",
              publicKey: "",
              rpId: location.hostname,
              origin: location.origin,
            })
            .signer({} as any);
      } catch (error) {
        return { message: (error as Error).message, calls };
      }
    }, operation);
    expect(result?.calls).toBe(1);
    expect(result?.message).toMatch(/cancelled|timed out/i);
    expect(result?.message).toMatch(/try again/i);
    expect(result?.message).toMatch(/recovery/i);
    expect(result?.message).not.toContain("Raw browser error");
  });
}

test("discovery ignores malformed local hints and preserves non-cancellation errors", async ({
  page,
}) => {
  const message = await page.evaluate(async () => {
    localStorage.setItem("keychain.passkey", "invalid JSON");
    navigator.credentials.get = async () => {
      throw new DOMException("RP configuration invalid", "SecurityError");
    };
    try {
      await (
        await import(/* @vite-ignore */ String("/identities.js"))
      ).discoverPasskey();
    } catch (error) {
      return (error as Error).message;
    }
  });
  expect(message).toBe("RP configuration invalid");
});
