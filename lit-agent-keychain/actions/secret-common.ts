import {
  manifestSchema,
  signedSchema,
  policySchema,
  envelopeSchema,
  signedRequestSchema,
  MAX_POLICY_SECONDS,
  V,
  type Manifest,
  type ReadRequest,
  type KeyBinding,
  type ProtectedResponse,
} from "../protocol/schema.ts";
import {
  digest,
  requireThat,
  verifyReceipt,
  verifyAgent,
  unhex,
  encryptionKey,
  encryptionPublicKey,
  decryptEnvelope,
  randomId,
  nowSeconds,
  seal,
  signAction,
  responseContext,
} from "../protocol/crypto.ts";
import { verifyWindow } from "../protocol/identity.ts";
import { jsonFetch } from "../protocol/http.ts";
import type { LitRuntime } from "./types.ts";
declare const Lit: LitRuntime;

type Use = (
  value: Uint8Array<ArrayBuffer>,
  request: ReadRequest,
) => Promise<Uint8Array<ArrayBuffer>>;
export async function execute(
  raw: Manifest,
  params: any,
  release: Manifest["release"],
  operation: ReadRequest["operation"],
  use: Use,
) {
  let actionKey: Uint8Array<ArrayBuffer> | undefined;
  let key: Uint8Array<ArrayBuffer> | undefined;
  let plaintext: Uint8Array<ArrayBuffer> | undefined;
  let output: Uint8Array<ArrayBuffer> | undefined;
  try {
    const manifest = manifestSchema.parse(raw);
    requireThat(manifest.release === release);
    if (params?.operation === "publicKey") {
      requireThat(
        typeof params.challenge === "string" &&
          /^[0-9a-f]{64}$/.test(params.challenge),
      );
      actionKey = unhex(
        (await Lit.Actions.getLitActionPrivateKey()).replace(/^0x/, ""),
      );
      key = encryptionKey(actionKey);
      const payload: KeyBinding["payload"] = {
        v: V,
        domain: "lit-keychain/key-binding/v2",
        manifestHash: digest(manifest),
        encryptionPublicKey: encryptionPublicKey(key),
        challenge: params.challenge,
      };
      return {
        ok: true,
        binding: { payload, signature: signAction(payload, actionKey) },
      };
    }
    requireThat(params?.operation === operation);
    const signedRequest = signedRequestSchema.parse(params.signedRequest);
    const request = signedRequest.request;
    const now = nowSeconds();
    requireThat(
      request.operation === operation &&
        request.secretId === manifest.secretId &&
        request.vaultId === manifest.vaultId,
    );
    verifyWindow(request.issuedAt, request.expiresAt, now, 120);
    verifyAgent(request, signedRequest.signature, request.agentPublicKey);
    const state = await jsonFetch(
      `${manifest.registry}/api/registry/secrets/${manifest.secretId}`,
      {},
      8000,
      256 * 1024,
    );
    const policy = signedSchema(policySchema).parse(state.policy);
    const authorityKey = await Lit.Actions.getLitActionPublicKey({
      ipfsId: manifest.authorityCid,
    });
    verifyReceipt(policy, authorityKey, manifest.vaultId);
    const p = policy.document;
    requireThat(
      p.vaultId === manifest.vaultId &&
        p.secretId === manifest.secretId &&
        p.actionCid === request.actionCid &&
        digest(p) === request.policyHash &&
        !p.disabled,
    );
    verifyWindow(p.notBefore, p.expiresAt, now, MAX_POLICY_SECONDS);
    requireThat(p.notBefore <= now);
    requireThat(
      p.grants.some(
        (g) =>
          g.agentPublicKey === request.agentPublicKey &&
          g.operations.includes(operation) &&
          g.versions.some(
            (v) =>
              v.version === request.version &&
              v.envelopeHash === request.envelopeHash,
          ),
      ),
    );
    const signedEnvelope = signedSchema(envelopeSchema).parse(params.envelope);
    verifyReceipt(signedEnvelope, authorityKey, manifest.vaultId);
    const envelope = signedEnvelope.document;
    requireThat(
      digest(envelope) === request.envelopeHash &&
        envelope.vaultId === manifest.vaultId &&
        envelope.metadata.vaultId === manifest.vaultId &&
        envelope.metadata.secretId === manifest.secretId &&
        envelope.metadata.actionCid === request.actionCid &&
        envelope.metadata.version === request.version &&
        envelope.metadata.release === manifest.release,
    );
    // Do not request the decryption key until all independently verified checks pass.
    actionKey = unhex(
      (await Lit.Actions.getLitActionPrivateKey()).replace(/^0x/, ""),
    );
    requireThat(nowSeconds() < request.expiresAt && nowSeconds() < p.expiresAt);
    key = encryptionKey(actionKey);
    plaintext = await decryptEnvelope(envelope, key);
    requireThat(nowSeconds() < request.expiresAt && nowSeconds() < p.expiresAt);
    output = await use(plaintext, request);
    const payload: ProtectedResponse["payload"] = {
      v: V,
      domain: "lit-keychain/response/v2",
      requestHash: digest(request),
      sealed: await seal(
        request.responsePublicKey,
        output,
        responseContext(request),
      ),
    };
    return {
      ok: true,
      result: { payload, signature: signAction(payload, actionKey) },
    };
  } catch {
    return { ok: false, error: "access_denied" };
  } finally {
    actionKey?.fill(0);
    key?.fill(0);
    plaintext?.fill(0);
    output?.fill(0);
  }
}
