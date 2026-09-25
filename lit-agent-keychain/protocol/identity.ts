import { p256 } from "@noble/curves/nist.js";
import { sha256 } from "@noble/hashes/sha2.js";
import { createLocalJWKSet, jwtVerify } from "jose";
import { recoverTypedDataAddress } from "viem";
import {
  challengeSchema,
  googleSessionSchema,
  ownerSchema,
  type Authority,
  type Challenge,
  type OwnerProof,
} from "./schema.ts";
import {
  b64u,
  canonical,
  digest,
  unhex,
  unb64u,
  utf8,
  requireThat,
  verifyAgent,
} from "./crypto.ts";
import { jsonFetch } from "./http.ts";

export function authorizationTypedData(challenge: Challenge) {
  return {
    domain: {
      name: "Lit Agent Keychain",
      version: "2",
      salt: `0x${challenge.vaultId}` as `0x${string}`,
    },
    types: {
      KeychainAuthorization: [
        { name: "operation", type: "string" },
        { name: "objectHash", type: "bytes32" },
        { name: "nonce", type: "bytes32" },
        { name: "issuedAt", type: "uint64" },
        { name: "expiresAt", type: "uint64" },
      ],
    },
    primaryType: "KeychainAuthorization" as const,
    message: {
      operation: challenge.operation,
      objectHash: `0x${challenge.objectHash}` as `0x${string}`,
      nonce: `0x${challenge.nonce}` as `0x${string}`,
      issuedAt: BigInt(challenge.issuedAt),
      expiresAt: BigInt(challenge.expiresAt),
    },
  };
}
export function verifyWindow(
  issuedAt: number,
  expiresAt: number,
  now: number,
  maxLifetime: number,
) {
  requireThat(
    Number.isSafeInteger(issuedAt) &&
      Number.isSafeInteger(expiresAt) &&
      issuedAt <= now + 30 &&
      expiresAt > now &&
      expiresAt > issuedAt &&
      expiresAt - issuedAt <= maxLifetime,
  );
}
export async function verifyOwnerProof(
  authority: Authority,
  proof: OwnerProof,
  object: { kind: string },
  now: number,
) {
  const owner = ownerSchema.parse(proof.owner);
  requireThat(owner.kind === proof.kind);
  const challenge = challengeSchema.parse(proof.challenge);
  requireThat(
    challenge.vaultId === digest(authority) &&
      challenge.objectHash === digest(object) &&
      challenge.operation === object.kind,
  );
  verifyWindow(challenge.issuedAt, challenge.expiresAt, now, 300);
  if (proof.kind === "wallet" && owner.kind === "wallet") {
    requireThat(
      typeof proof.signature === "string" &&
        /^0x[0-9a-fA-F]{130}$/.test(proof.signature),
    );
    const address = await recoverTypedDataAddress({
      ...authorizationTypedData(challenge),
      signature: proof.signature,
    });
    requireThat(address.toLowerCase() === owner.address);
  } else if (proof.kind === "passkey" && owner.kind === "passkey") {
    const origin = new URL(owner.origin);
    requireThat(
      origin.hostname === owner.rpId ||
        origin.hostname.endsWith("." + owner.rpId),
    );
    const clientBytes = unb64u(proof.clientDataJSON, 4096);
    const client = JSON.parse(
      new TextDecoder("utf-8", { fatal: true }).decode(clientBytes),
    );
    requireThat(
      client.type === "webauthn.get" &&
        client.origin === owner.origin &&
        client.challenge === b64u(unhex(digest(challenge))) &&
        client.crossOrigin !== true &&
        !client.topOrigin,
    );
    const auth = unb64u(proof.authenticatorData, 1024);
    requireThat(
      auth.length >= 37 &&
        (auth[32] & 0x05) === 0x05 &&
        (auth[32] & 0x02) === 0 &&
        (auth[32] & 0x40) === 0,
    );
    const expected = sha256(utf8(owner.rpId));
    requireThat(auth.subarray(0, 32).every((b, i) => b === expected[i]));
    const signed = new Uint8Array(auth.length + 32);
    signed.set(auth);
    signed.set(sha256(clientBytes), auth.length);
    requireThat(
      p256.verify(unb64u(proof.signature, 80), signed, unhex(owner.publicKey), {
        format: "der",
        lowS: false,
      }),
    );
  } else if (proof.kind === "google" && owner.kind === "google") {
    const session = googleSessionSchema.parse(proof.session);
    requireThat(
      session.network === authority.network &&
        session.registry === authority.registry,
    );
    verifyWindow(session.issuedAt, session.expiresAt, now, 3600);
    requireThat(challenge.expiresAt <= session.expiresAt);
    requireThat(typeof proof.token === "string" && proof.token.length <= 8192);
    const jwks = await jsonFetch(
      "https://www.googleapis.com/oauth2/v3/certs",
      {},
      8000,
      65536,
    );
    requireThat(Array.isArray(jwks.keys) && jwks.keys.length <= 16);
    const { payload, protectedHeader } = await jwtVerify(
      proof.token,
      createLocalJWKSet(jwks),
      {
        algorithms: ["RS256"],
        audience: owner.clientId,
        subject: owner.subject,
        issuer: ["https://accounts.google.com", "accounts.google.com"],
        currentDate: new Date(now * 1000),
        clockTolerance: 0,
        requiredClaims: ["sub", "iss", "aud", "exp", "iat", "nonce"],
      },
    );
    requireThat(
      typeof protectedHeader.kid === "string" &&
        protectedHeader.kid.length <= 128 &&
        payload.aud === owner.clientId &&
        payload.nonce === b64u(unhex(digest(session))) &&
        typeof payload.iat === "number" &&
        Number.isSafeInteger(payload.iat) &&
        payload.iat <= now + 30 &&
        typeof payload.exp === "number" &&
        Number.isSafeInteger(payload.exp) &&
        payload.exp - payload.iat <= 7200 &&
        session.expiresAt <= payload.exp &&
        (payload.azp === undefined || payload.azp === owner.clientId),
    );
    verifyAgent(challenge, proof.signature, session.publicKey);
  } else throw new Error("Unsupported owner proof");
  return owner;
}
export const sameOwner = (a: unknown, b: unknown) =>
  canonical(a) === canonical(b);
