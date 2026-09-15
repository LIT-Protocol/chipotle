import { secp256k1 } from "@noble/curves/secp256k1.js";
import {
  authoritySchema,
  credentialsSchema,
  documentSchema,
  signedSchema,
  MAX_POLICY_SECONDS,
  type Authority,
  type OwnerProof,
} from "../protocol/schema.ts";
import {
  digest,
  hex,
  unhex,
  requireThat,
  verifyReceipt,
  makeReceipt,
  nowSeconds,
} from "../protocol/crypto.ts";
import { jsonFetch } from "../protocol/http.ts";
import {
  sameOwner,
  verifyOwnerProof,
  verifyWindow,
} from "../protocol/identity.ts";
import type { LitRuntime } from "./types.ts";
declare const Lit: LitRuntime;

export async function run(
  rawAuthority: Authority,
  params: { document: unknown; proof: OwnerProof },
) {
  let privateKey: Uint8Array | undefined;
  try {
    const authority = authoritySchema.parse(rawAuthority);
    const document = documentSchema.parse(params.document);
    const vaultId = digest(authority);
    const now = nowSeconds();
    requireThat(document.vaultId === vaultId);
    const signer = await verifyOwnerProof(
      authority,
      params.proof,
      document,
      now,
    );
    privateKey = unhex(
      (await Lit.Actions.getLitActionPrivateKey()).replace(/^0x/, ""),
    );
    const publicKey = hex(secp256k1.getPublicKey(privateKey));
    // Null denotes the original credential. Missing/failed HTTP responses fail closed.
    const state = await jsonFetch(
      `${authority.registry}/api/registry/credentials/${vaultId}`,
      {},
      8000,
      65536,
    );
    let owners = [authority.owner];
    let credentialExpiresAt: number | null = null;
    if (state.policy !== null) {
      const signed = signedSchema(credentialsSchema).parse(state.policy);
      verifyReceipt(signed, publicKey, vaultId);
      requireThat(signed.document.notBefore <= now);
      if (signed.document.expiresAt !== null)
        verifyWindow(
          signed.document.notBefore,
          signed.document.expiresAt,
          now,
          366 * 86400,
        );
      owners = signed.document.owners;
      credentialExpiresAt = signed.document.expiresAt;
    }
    requireThat(owners.some((o) => sameOwner(o, signer)));
    if (document.kind === "policy") {
      verifyWindow(
        document.notBefore,
        document.expiresAt,
        now,
        MAX_POLICY_SECONDS,
      );
      requireThat(
        document.grants.every(
          (g) => new Set(g.operations).size === g.operations.length,
        ),
      );
    }
    if (document.kind === "credentials") {
      requireThat(document.notBefore <= now);
      if (document.expiresAt !== null)
        verifyWindow(document.notBefore, document.expiresAt, now, 366 * 86400);
    }
    if (document.kind === "login")
      requireThat(document.expiresAt > now && document.expiresAt <= now + 300);
    if (document.kind === "envelope")
      requireThat(document.metadata.vaultId === vaultId);
    if (document.kind === "manifest")
      requireThat(
        document.manifest.vaultId === vaultId &&
          document.manifest.registry === authority.registry &&
          document.manifest.network === authority.network,
      );
    const issuedAt = nowSeconds();
    requireThat(
      issuedAt < params.proof.challenge.expiresAt &&
        (credentialExpiresAt === null || issuedAt < credentialExpiresAt),
    );
    if (document.kind === "policy" || document.kind === "login")
      requireThat(issuedAt < document.expiresAt);
    if (params.proof.kind === "google")
      requireThat(issuedAt < params.proof.session.expiresAt);
    return { ok: true, receipt: makeReceipt(document, privateKey, issuedAt) };
  } catch {
    return { ok: false, error: "authorization_denied" };
  } finally {
    privateKey?.fill(0);
  }
}
