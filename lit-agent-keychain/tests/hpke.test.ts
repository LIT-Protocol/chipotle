import { test } from "node:test";
import assert from "node:assert/strict";
import { CipherSuite, Aes256Gcm, HkdfSha256 } from "@hpke/core";
import { DhkemX25519HkdfSha256 } from "@hpke/dhkem-x25519";
import vector from "./fixtures/hpke-rfc9180.json";
import { unhex, hex } from "../protocol/crypto.ts";
test("HPKE implementation matches RFC9180 independent X25519/SHA256/AES256GCM vectors", async () => {
  const suite = new CipherSuite({
    kem: new DhkemX25519HkdfSha256(),
    kdf: new HkdfSha256(),
    aead: new Aes256Gcm(),
  });
  const recipientKey = await suite.kem.deserializePrivateKey(
    unhex(vector.skRm).buffer,
  );
  const context = await suite.createRecipientContext({
    recipientKey,
    enc: unhex(vector.enc).buffer,
    info: unhex(vector.info).buffer,
  });
  for (const encrypted of vector.encryptions) {
    const result = await context.open(
      unhex(encrypted.ct).buffer,
      unhex(encrypted.aad).buffer,
    );
    assert.equal(hex(new Uint8Array(result)), encrypted.pt);
  }
});
