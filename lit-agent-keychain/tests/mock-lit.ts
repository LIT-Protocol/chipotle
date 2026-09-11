// Test-only boundary adapter: real bundled actions and WebCrypto, deterministic
// synthetic action keys, actual registry HTTP. Never use this server in production.
import http from "node:http";
import { Harness, pubFor } from "./harness.ts";
import { cidForCode } from "../protocol/actions.ts";
import { generateKeyPair, exportJWK, SignJWT } from "jose";
const port = Number(process.env.KEYCHAIN_MOCK_LIT_PORT || 55440);
const keys = await generateKeyPair("RS256", { modulusLength: 2048 });
const jwk = {
  ...(await exportJWK(keys.publicKey)),
  kid: "keychain-test-google",
  alg: "RS256",
  use: "sig",
};
const nativeFetch = globalThis.fetch;
const server = http.createServer(async (req, res) => {
  res.setHeader("Content-Type", "application/json");
  res.setHeader("Access-Control-Allow-Origin", "*");
  try {
    const url = new URL(req.url || "/", `http://127.0.0.1:${port}`);
    if (url.pathname.startsWith("/core/v1/lit_action_public_key/")) {
      const cid = url.pathname.split("/").at(-1)!;
      res.end(JSON.stringify({ public_key: "0x" + pubFor(cid) }));
      return;
    }
    if (url.pathname === "/test/google-token") {
      const nonce = url.searchParams.get("nonce");
      const now = Math.floor(Date.now() / 1000);
      const token = await new SignJWT({
        iss: "https://accounts.google.com",
        aud: "test.apps.googleusercontent.com",
        sub: url.searchParams.get("sub") || "123456789",
        iat: now,
        exp: now + 3600,
        nonce,
      })
        .setProtectedHeader({ alg: "RS256", kid: jwk.kid })
        .sign(keys.privateKey);
      res.end(JSON.stringify({ token }));
      return;
    }
    if (url.pathname !== "/core/v1/lit_action" || req.method !== "POST") {
      res.writeHead(404);
      res.end("{}");
      return;
    }
    if (req.headers["x-privacy-mode"] !== "true")
      throw new Error("Missing privacy mode");
    let raw = "";
    for await (const chunk of req) {
      raw += chunk;
      if (raw.length > 2 * 1024 * 1024) throw new Error("too large");
    }
    const body = JSON.parse(raw);
    const cid = await cidForCode(body.code);
    const h = new Harness();
    h.fetch = async (input, init) => {
      const target = String(input);
      if (target === "https://www.googleapis.com/oauth2/v3/certs")
        return new Response(JSON.stringify({ keys: [jwk] }), {
          headers: { "content-type": "application/json" },
        });
      if (
        !["127.0.0.1", "localhost"].includes(new URL(target).hostname) ||
        new URL(target).protocol !== "http:"
      )
        throw new Error("Test egress denied");
      return nativeFetch(input, init);
    };
    const response = await h.runCode(body.code, cid, body.js_params);
    res.end(
      JSON.stringify({
        response,
        logs: "TEST-ONLY-UPSTREAM-LOG-MUST-NOT-REACH-CLIENT",
        has_error: false,
      }),
    );
  } catch {
    res.writeHead(500);
    res.end(JSON.stringify({ error: "mock_execution_failed" }));
  }
});
server.listen(port, "127.0.0.1", () =>
  process.stdout.write(`Test Lit adapter listening on ${port}\n`),
);
