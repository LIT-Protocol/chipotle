// Test-only boundary adapter: real bundled actions and WebCrypto, deterministic
// synthetic action keys, actual registry HTTP. Never use this server in production.
import http from "node:http";
import { Harness } from "./harness.ts";
import { keccak256, toHex } from "viem";
import { randomBytes } from "node:crypto";
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
const groups = new Map<number, Set<string>>();
const usageKeys = new Map<string, number[]>();
let nextGroup = 1;
let failRemoval = false;
const server = http.createServer(async (req, res) => {
  res.setHeader("Content-Type", "application/json");
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader(
    "Access-Control-Allow-Headers",
    "Content-Type, X-Api-Key, X-Privacy-Mode",
  );
  res.setHeader("Access-Control-Allow-Methods", "GET, POST, OPTIONS");
  try {
    if (req.method === "OPTIONS") {
      res.writeHead(204);
      res.end();
      return;
    }
    const url = new URL(req.url || "/", `http://127.0.0.1:${port}`);
    if (url.pathname === "/test/fail-removal") {
      failRemoval = true;
      res.end("{}");
      return;
    }
    if (url.pathname === "/core/v1/list_api_keys") {
      if (req.headers["x-api-key"] !== "local-master-only") {
        res.writeHead(403);
        res.end("{}");
        return;
      }
      const page = Number(url.searchParams.get("page_number"));
      const size = Number(url.searchParams.get("page_size"));
      res.end(
        JSON.stringify(
          [...usageKeys.keys()]
            .slice(page * size, (page + 1) * size)
            .map((key) => ({ api_key_hash: keccak256(toHex(key)) })),
        ),
      );
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
    const management = [
      "add_group",
      "update_group",
      "add_action_to_group",
      "update_usage_api_key",
      "add_usage_api_key",
      "remove_usage_api_key",
    ].includes(url.pathname.split("/").at(-1)!);
    if (
      (!management && url.pathname !== "/core/v1/lit_action") ||
      req.method !== "POST"
    ) {
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
    if (management) {
      if (req.headers["x-api-key"] !== "local-master-only") {
        res.writeHead(403);
        res.end("{}");
        return;
      }
      let result: any = { success: true };
      if (url.pathname.endsWith("/add_group")) {
        if (body.cid_hashes_permitted.length > 10)
          throw new Error("Group CID limit");
        const group = nextGroup++;
        groups.set(group, new Set(body.cid_hashes_permitted));
        result.group_id = String(group);
      }
      if (url.pathname.endsWith("/update_group")) {
        if (
          !groups.has(body.group_id) ||
          body.pkp_ids_permitted.length ||
          body.cid_hashes_permitted.length > 10
        )
          throw new Error("Invalid group");
        groups.set(body.group_id, new Set(body.cid_hashes_permitted));
      }
      if (url.pathname.endsWith("/add_action_to_group")) {
        const group = groups.get(body.group_id);
        if (!group) throw new Error("Invalid group");
        group.add(keccak256(toHex(body.action_ipfs_cid)));
      }
      if (
        url.pathname.endsWith("/add_usage_api_key") ||
        url.pathname.endsWith("/update_usage_api_key")
      ) {
        if (
          body.can_create_groups ||
          body.can_delete_groups ||
          body.can_create_pkps ||
          body.manage_ipfs_ids_in_groups.length ||
          body.add_pkp_to_groups.length ||
          body.remove_pkp_from_groups.length ||
          body.execute_in_groups.length < 1 ||
          body.execute_in_groups.length > 2 ||
          !body.execute_in_groups.every((g: number) => groups.has(g))
        )
          throw new Error("Overbroad permissions");
        const key = url.pathname.endsWith("/update_usage_api_key")
          ? body.usage_api_key
          : randomBytes(32).toString("base64");
        if (body.usage_api_key && !usageKeys.has(key))
          throw new Error("Missing usage key");
        usageKeys.set(key, body.execute_in_groups);
        result.usage_api_key = key;
      }
      if (url.pathname.endsWith("/remove_usage_api_key")) {
        usageKeys.delete(body.usage_api_key);
        if (failRemoval) {
          failRemoval = false;
          res.writeHead(500);
          res.end("{}");
          return;
        }
      }
      res.end(JSON.stringify(result));
      return;
    }
    const cid = await cidForCode(body.code);
    if (req.headers["x-api-key"] !== "local-test-only") {
      const permitted = usageKeys.get(req.headers["x-api-key"] as string);
      if (
        !permitted?.some((group) =>
          groups.get(group)?.has(keccak256(toHex(cid))),
        )
      ) {
        res.writeHead(403);
        res.end(JSON.stringify({ error: "not_authorized" }));
        return;
      }
    }
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
  } catch (error) {
    if (process.env.KEYCHAIN_ADAPTER_DEBUG)
      console.error("Test adapter failure", error);
    res.writeHead(500);
    res.end(JSON.stringify({ error: "mock_execution_failed" }));
  }
});
server.listen(port, "127.0.0.1", () =>
  process.stdout.write(`Test Lit adapter listening on ${port}\n`),
);
