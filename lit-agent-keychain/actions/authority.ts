import { secp256k1 } from "@noble/curves/secp256k1.js";
import {
  authoritySchema,
  batchSchema,
  credentialsSchema,
  documentSchema,
  signedSchema,
  MAX_POLICY_SECONDS,
  type Authority,
  type Document,
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
  params: { document?: unknown; documents?: unknown; proof: OwnerProof },
) {
  let privateKey: Uint8Array | undefined;
  let publicKey: string | undefined;
  try {
    const authority = authoritySchema.parse(rawAuthority);
    const vaultId = digest(authority);
    const now = nowSeconds();
    // One proof covers either a single document (any kind) or a batch of secret
    // objects. The owner signs the digest of exactly what is approved; every
    // document in a batch still gets its own exact-object receipt.
    requireThat(
      (params.document === undefined) !== (params.documents === undefined),
    );
    let documents: Document[];
    let signedObject: { kind: string };
    if (params.documents !== undefined) {
      const batch = batchSchema.parse({
        kind: "batch",
        vaultId,
        documents: params.documents,
      });
      documents = batch.documents;
      signedObject = batch;
    } else {
      const document = documentSchema.parse(params.document);
      documents = [document];
      signedObject = document;
    }
    requireThat(documents.every((d) => d.vaultId === vaultId));
    const signer = await verifyOwnerProof(
      authority,
      params.proof,
      signedObject,
      now,
    );
    privateKey = unhex(
      (await Lit.Actions.getLitActionPrivateKey()).replace(/^0x/, ""),
    );
    publicKey = hex(secp256k1.getPublicKey(privateKey));
    // Null denotes the original credential. Missing/failed HTTP responses fail closed.
    const state = await jsonFetch(
      `${authority.registry}/api/registry/credentials/${vaultId}`,
      {},
      8000,
      65536,
    );
    let owners = [authority.owner];
    let credentialExpiresAt: number | null = null;
    // Authority releases are versioned: each vault may have credentials receipts
    // signed by an earlier or later release of this action, whose key this
    // release cannot verify. Such a receipt is treated exactly like a null state
    // (root owner only). The operator can already serve null for any vault, so
    // this grants nothing beyond the documented rollback trust; it never accepts
    // an unverifiable owner set.
    if (state.policy !== null && signedByThisRelease(state.policy)) {
      const signed = signedSchema(credentialsSchema).parse(state.policy);
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
    for (const document of documents)
      checkDocument(document, authority, vaultId, now);
    const issuedAt = nowSeconds();
    requireThat(
      issuedAt < params.proof.challenge.expiresAt &&
        (credentialExpiresAt === null || issuedAt < credentialExpiresAt),
    );
    for (const document of documents)
      if (document.kind === "policy" || document.kind === "login")
        requireThat(issuedAt < document.expiresAt);
    if (params.proof.kind === "google")
      requireThat(issuedAt < params.proof.session.expiresAt);
    const receipts = documents.map((document) =>
      makeReceipt(document, privateKey!, issuedAt),
    );
    return params.documents !== undefined
      ? { ok: true, receipts }
      : { ok: true, receipt: receipts[0] };
  } catch {
    return { ok: false, error: "authorization_denied" };
  } finally {
    privateKey?.fill(0);
  }
  function signedByThisRelease(policy: unknown) {
    try {
      const signed = signedSchema(credentialsSchema).parse(policy);
      verifyReceipt(
        signed,
        publicKey!,
        digest(authoritySchema.parse(rawAuthority)),
      );
      return true;
    } catch {
      return false;
    }
  }
}
/** Per-kind invariants; identical whether the document arrived alone or in a batch. */
function checkDocument(
  document: Document,
  authority: Authority,
  vaultId: string,
  now: number,
) {
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
}
