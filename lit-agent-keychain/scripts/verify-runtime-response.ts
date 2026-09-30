import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import {
  open,
  unhex,
  decode,
  responseContext,
  verifyAction,
} from "../protocol/crypto.ts";
import { pubFor } from "../tests/harness.ts";
const fixture = JSON.parse(
  await readFile("generated/runtime-vector.json", "utf8"),
);
const response = JSON.parse(
  await readFile("generated/runtime-vector.json.response", "utf8"),
);
verifyAction(
  response.result.payload,
  response.result.signature,
  pubFor(fixture.request.actionCid),
);
assert.equal(
  decode(
    await open(
      unhex(fixture.responseKey),
      response.result.payload.sealed,
      responseContext(fixture.request),
    ),
  ),
  fixture.expected,
);
process.stdout.write(
  "Actual Deno runtime response signature and HPKE decryption verified\n",
);
