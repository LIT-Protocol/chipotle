import { writeFile, mkdir } from "node:fs/promises";
import { cidForCode } from "../protocol/actions.ts";
import { makeReceipt, hex, unhex } from "../protocol/crypto.ts";
import { secp256k1 } from "@noble/curves/secp256k1.js";
await mkdir("tests/fixtures", { recursive: true });
const privateKey = unhex("11".repeat(32));
const document = {
  v: 2,
  domain: "lit-keychain/v2",
  vaultId: "22".repeat(32),
  kind: "login",
  challenge: "33".repeat(32),
  expiresAt: 2000000000,
};
const cases = [];
for (const [pattern, repeat] of [
  ["abc", 1],
  ["é☃", 100000],
  ["x", 262144],
  ["x", 262145],
] as const)
  cases.push({
    pattern,
    repeat,
    cid: await cidForCode(pattern.repeat(repeat)),
  });
await writeFile(
  "tests/fixtures/protocol.json",
  JSON.stringify(
    {
      publicKey: hex(secp256k1.getPublicKey(privateKey)),
      signed: { document, receipt: makeReceipt(document, privateKey, 1) },
      cids: cases,
    },
    null,
    2,
  ) + "\n",
);
