import { z } from "zod";
import {
  releaseIdSchema,
  operationSchema,
  MAX_INPUT_BYTES,
} from "@lit-protocol/agent-keychain-library/shape";

export const V = 2 as const;
export const DOMAIN = "lit-keychain/v2" as const;
export const MAX_SECRET_BYTES = 16 * 1024;
export const MAX_POLICY_SECONDS = 90 * 86400;
const hex = (bytes: number) =>
  z.string().regex(new RegExp(`^[0-9a-f]{${bytes * 2}}$`));
export const hashSchema = hex(32);
export const cidSchema = z.string().regex(/^Qm[1-9A-HJ-NP-Za-km-z]{44}$/);
const integer = z.number().int().min(0).max(Number.MAX_SAFE_INTEGER);
const text = z.string().min(1).max(128);
export const originSchema = z
  .string()
  .max(256)
  .refine((value) => {
    try {
      const u = new URL(value);
      return (
        u.origin === value &&
        (u.protocol === "https:" ||
          (u.protocol === "http:" &&
            ["localhost", "127.0.0.1", "[::1]"].includes(u.hostname)))
      );
    } catch {
      return false;
    }
  }, "Expected an HTTPS origin (HTTP only on loopback)");
export const ownerSchema = z.discriminatedUnion("kind", [
  z.strictObject({
    kind: z.literal("wallet"),
    address: z.string().regex(/^0x[0-9a-f]{40}$/),
  }),
  z.strictObject({
    kind: z.literal("passkey"),
    publicKey: z.string().regex(/^04[0-9a-f]{128}$/),
    credentialId: z
      .string()
      .min(1)
      .max(1400)
      .regex(/^[A-Za-z0-9_-]+$/),
    rpId: z
      .string()
      .min(1)
      .max(253)
      .regex(/^[a-z0-9.-]+$/),
    origin: originSchema,
  }),
  z.strictObject({
    kind: z.literal("google"),
    subject: z.string().min(1).max(255),
    clientId: z
      .string()
      .min(1)
      .max(256)
      .regex(/^[A-Za-z0-9_-]+\.apps\.googleusercontent\.com$/),
  }),
]);
export type Owner = z.infer<typeof ownerSchema>;
export const authoritySchema = z.strictObject({
  v: z.literal(V),
  network: text,
  registry: originSchema,
  owner: ownerSchema,
});
export type Authority = z.infer<typeof authoritySchema>;
export const manifestSchema = z.strictObject({
  v: z.literal(V),
  network: text,
  registry: originSchema,
  vaultId: hashSchema,
  authorityCid: cidSchema,
  secretId: hashSchema,
  release: releaseIdSchema,
});
export type Manifest = z.infer<typeof manifestSchema>;
const objectBase = {
  v: z.literal(V),
  domain: z.literal(DOMAIN),
  vaultId: hashSchema,
};
export const metadataSchema = z.strictObject({
  ...objectBase,
  secretId: hashSchema,
  actionCid: cidSchema,
  version: integer.min(1),
  release: releaseIdSchema,
  name: z.string().regex(/^[A-Z][A-Z0-9_]{0,63}$/),
});
const b64 = z.string().regex(/^[A-Za-z0-9_-]+$/);
export const sealedSchema = z.strictObject({
  enc: b64.max(64),
  ciphertext: b64.max(24 * 1024),
});
export const envelopeSchema = z.strictObject({
  ...objectBase,
  kind: z.literal("envelope"),
  metadata: metadataSchema,
  enc: b64.max(64),
  wrappedKey: b64.max(96),
  iv: b64.max(24),
  ciphertext: b64.max(24 * 1024),
});
export type Envelope = z.infer<typeof envelopeSchema>;
const versionSchema = z.strictObject({
  version: integer.min(1),
  envelopeHash: hashSchema,
});
export const grantSchema = z.strictObject({
  agentPublicKey: hex(32),
  label: text,
  operations: z.array(operationSchema).min(1).max(4),
  versions: z.array(versionSchema).min(1).max(100),
});
export type Grant = z.infer<typeof grantSchema>;
export const policySchema = z.strictObject({
  ...objectBase,
  kind: z.literal("policy"),
  secretId: hashSchema,
  actionCid: cidSchema,
  epoch: integer.min(1),
  previousHash: hashSchema.nullable(),
  disabled: z.boolean(),
  notBefore: integer,
  expiresAt: integer,
  grants: z.array(grantSchema).max(100),
});
export type Policy = z.infer<typeof policySchema>;
export const credentialsSchema = z.strictObject({
  ...objectBase,
  kind: z.literal("credentials"),
  epoch: integer.min(1),
  previousHash: hashSchema.nullable(),
  owners: z.array(ownerSchema).min(1).max(8),
  notBefore: integer,
  expiresAt: integer.nullable(),
});
export type Credentials = z.infer<typeof credentialsSchema>;
export const manifestDocumentSchema = z.strictObject({
  ...objectBase,
  kind: z.literal("manifest"),
  manifest: manifestSchema,
  actionCid: cidSchema,
});
export const loginSchema = z.strictObject({
  ...objectBase,
  kind: z.literal("login"),
  challenge: hashSchema,
  expiresAt: integer,
});
export const documentSchema = z.discriminatedUnion("kind", [
  envelopeSchema,
  policySchema,
  credentialsSchema,
  manifestDocumentSchema,
  loginSchema,
]);
export type Document = z.infer<typeof documentSchema>;
/**
 * Several documents approved with one owner signature. The owner signs the
 * digest of this wrapper (operation `batch`) and the authority action issues an
 * ordinary per-document receipt for each entry, so consumers are unchanged. Only
 * secret objects may be batched; sign-in and owner-credential changes stay single.
 */
export const BATCH_KINDS = ["manifest", "envelope", "policy"] as const;
export const batchSchema = z.strictObject({
  kind: z.literal("batch"),
  vaultId: hashSchema,
  documents: z
    .array(
      z.discriminatedUnion("kind", [
        envelopeSchema,
        policySchema,
        manifestDocumentSchema,
      ]),
    )
    .min(1)
    .max(8),
});
export type Batch = z.infer<typeof batchSchema>;
export const receiptSchema = z.strictObject({
  payload: z.strictObject({
    v: z.literal(V),
    domain: z.literal("lit-keychain/receipt/v2"),
    vaultId: hashSchema,
    objectHash: hashSchema,
    issuedAt: integer,
  }),
  signature: hex(64),
});
export type Receipt = z.infer<typeof receiptSchema>;
export type Signed<T> = { document: T; receipt: Receipt };
export const signedSchema = <T extends z.ZodType>(schema: T) =>
  z.strictObject({ document: schema, receipt: receiptSchema });
export const challengeSchema = z.strictObject({
  v: z.literal(V),
  domain: z.literal("lit-keychain/authorize/v2"),
  vaultId: hashSchema,
  objectHash: hashSchema,
  operation: documentSchema.options[0].shape.kind.or(
    z.enum(["policy", "credentials", "manifest", "login", "batch"]),
  ),
  nonce: hashSchema,
  issuedAt: integer,
  expiresAt: integer,
});
export type Challenge = z.infer<typeof challengeSchema>;
export const googleSessionSchema = z.strictObject({
  v: z.literal(V),
  domain: z.literal("lit-keychain/google-session/v2"),
  network: text,
  registry: originSchema,
  publicKey: hex(32),
  nonce: hashSchema,
  issuedAt: integer,
  expiresAt: integer,
  scope: z.literal("authorize"),
});
export type GoogleSession = z.infer<typeof googleSessionSchema>;
export type OwnerProof =
  | {
      kind: "wallet";
      owner: Extract<Owner, { kind: "wallet" }>;
      challenge: Challenge;
      signature: `0x${string}`;
    }
  | {
      kind: "passkey";
      owner: Extract<Owner, { kind: "passkey" }>;
      challenge: Challenge;
      clientDataJSON: string;
      authenticatorData: string;
      signature: string;
    }
  | {
      kind: "google";
      owner: Extract<Owner, { kind: "google" }>;
      challenge: Challenge;
      token: string;
      session: GoogleSession;
      signature: string;
    };
export const requestSchema = z.strictObject({
  ...objectBase,
  secretId: hashSchema,
  actionCid: cidSchema,
  version: integer.min(1),
  envelopeHash: hashSchema,
  policyHash: hashSchema,
  agentPublicKey: hex(32),
  operation: operationSchema,
  /** Action-specific input, validated in the action against its catalog shape. Canonical JSON, bounded. */
  input: z
    .record(z.string(), z.unknown())
    .refine(
      (value) => JSON.stringify(value).length <= MAX_INPUT_BYTES,
      "input too large",
    )
    .optional(),
  responsePublicKey: hex(32),
  nonce: hashSchema,
  issuedAt: integer,
  expiresAt: integer,
});
export type ReadRequest = z.infer<typeof requestSchema>;
export const signedRequestSchema = z.strictObject({
  request: requestSchema,
  signature: hex(64),
});
export type SignedRequest = z.infer<typeof signedRequestSchema>;
export type ProtectedResponse = {
  payload: {
    v: 2;
    domain: "lit-keychain/response/v2";
    requestHash: string;
    sealed: z.infer<typeof sealedSchema>;
  };
  signature: string;
};
export type KeyBinding = {
  payload: {
    v: 2;
    domain: "lit-keychain/key-binding/v2";
    manifestHash: string;
    encryptionPublicKey: string;
    challenge: string;
  };
  signature: string;
};
