// Runs from a separately installed npm tarball, never from the source tree.
import { test } from "node:test";
import assert from "node:assert/strict";
import tls from "node:tls";
import { syncBuiltinESMExports } from "node:module";
import { EventEmitter } from "node:events";
import { readFileSync } from "node:fs";
import { createHash, X509Certificate } from "node:crypto";
import {
  Keychain,
  LitConnection,
  CHIPOTLE_ATTESTATION_POLICY,
} from "@lit-protocol/keychain";

const read = (name) =>
  readFileSync(`${process.env.KEYCHAIN_TEST_FIXTURES}/${name}`, "utf8");
const raw = new X509Certificate(read("cert-api.chipotle.litprotocol.com.pem"))
  .raw;
const hash = createHash("sha256").update(raw).digest("hex");
const origin = "https://attested.example";
const config = { v: 2, litApiUrl: origin, secrets: {} };

test("Node defaults bind TLS, cache, refresh and reject mismatches", async () => {
  const connect = tls.connect,
    fetch = globalThis.fetch,
    now = Date.now;
  let handshakes = 0,
    cert = raw,
    clock = Date.UTC(2026, 8, 11);
  Date.now = () => clock;
  tls.connect = (options) => {
    handshakes++;
    assert.equal(options.host, "attested.example");
    const socket = new EventEmitter();
    socket.authorized = true;
    socket.getPeerCertificate = () => ({ raw: cert });
    socket.end = socket.destroy = () => {};
    queueMicrotask(() => socket.emit("secureConnect"));
    return socket;
  };
  syncBuiltinESMExports();
  globalThis.fetch = async (input) => {
    const url = String(input);
    if (url === CHIPOTLE_ATTESTATION_POLICY.rpcUrl)
      return Response.json({
        jsonrpc: "2.0",
        id: 1,
        result: "0x" + "0".repeat(63) + "1",
      });
    const files = {
      "/attestation": "attestation.json",
      "/info": "info.json",
      "/evidences/quote.json": "evidence-quote.json",
      "/evidences/sha256sum.txt": "sha256sum.txt",
      "/evidences/cert-api.chipotle.litprotocol.com.pem":
        "cert-api.chipotle.litprotocol.com.pem",
    };
    const file = files[new URL(url).pathname];
    assert.ok(file, url);
    return new Response(read(file));
  };
  const client = new Keychain(Keychain.generateKey().privateKey, config, {
    attestation: CHIPOTLE_ATTESTATION_POLICY,
  });
  try {
    assert.equal(handshakes, 0, "construction is lazy");
    const first = await client.attest();
    assert.ok(first.checks.includes("tls-certificate-in-tee"));
    assert.equal(handshakes, 1);
    assert.equal(await client.attest(), first);
    assert.equal(handshakes, 1);
    clock += 3600001;
    await client.attest();
    assert.equal(handshakes, 2, "refresh observes the current certificate");
    cert = Buffer.from("not the attested certificate");
    clock += 3600001;
    await assert.rejects(client.attest(), /TLS|certificate/i);
    cert = raw;
    assert.ok(
      (await client.attest()).checks.includes("tls-certificate-in-tee"),
      "failure can be retried",
    );
    const before = handshakes;
    const explicit = new LitConnection(
      origin,
      30000,
      undefined,
      CHIPOTLE_ATTESTATION_POLICY,
      { tlsCertificateSha256: hash },
    );
    assert.ok(
      (await explicit.attest()).checks.includes("tls-certificate-in-tee"),
    );
    assert.equal(handshakes, before, "explicit transport hook is preserved");
    assert.equal(
      await new LitConnection(origin, 30000, undefined, false).attest(),
      undefined,
    );
    assert.equal(
      await new LitConnection("https://unknown.example").attest(),
      undefined,
    );
    assert.equal(handshakes, before, "unattested endpoints do not probe TLS");
  } finally {
    client.destroy();
    tls.connect = connect;
    syncBuiltinESMExports();
    globalThis.fetch = fetch;
    Date.now = now;
  }
});

test("TLS errors fail closed before HTTP and may be retried", async () => {
  const connect = tls.connect,
    fetch = globalThis.fetch;
  let attempts = 0,
    requests = 0;
  tls.connect = () => {
    attempts++;
    throw new Error("TLS probe failed");
  };
  syncBuiltinESMExports();
  globalThis.fetch = async () => {
    requests++;
    throw new Error("unexpected HTTP");
  };
  try {
    const client = new Keychain(Keychain.generateKey().privateKey, {
      ...config,
      litApiUrl: "https://api.chipotle.litprotocol.com",
    });
    try {
      await assert.rejects(client.attest(), /TLS probe failed/);
      await assert.rejects(client.attest(), /TLS probe failed/);
    } finally {
      client.destroy();
    }
    assert.equal(attempts, 2);
    assert.equal(requests, 0);
  } finally {
    tls.connect = connect;
    syncBuiltinESMExports();
    globalThis.fetch = fetch;
  }
});
