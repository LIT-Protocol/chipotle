import {
  manifestSchema,
  signedSchema,
  policySchema,
  envelopeSchema,
  signedRequestSchema,
  V,
  type Manifest,
  type KeyBinding,
  type ProtectedResponse,
} from "../protocol/schema.ts";
import {
  digest,
  requireThat,
  verifyReceipt,
  verifyAgent,
  unhex,
  utf8,
  decode,
  encryptionKey,
  encryptionPublicKey,
  decryptEnvelope,
  nowSeconds,
  seal,
  signAction,
  responseContext,
} from "../protocol/crypto.ts";
import { verifyWindow } from "../protocol/identity.ts";
import { jsonFetch, textFetch } from "../protocol/http.ts";
import {
  shapeToZod,
  MAX_OUTPUT_BYTES,
} from "@lit-protocol/agent-keychain-library/shape";
// Type-only: the definition schema itself is never bundled into a template.
import type {
  ActionDefinition,
  UseDefinition,
} from "@lit-protocol/agent-keychain-library/schema";
import type {
  ActionRequestInit,
  ActionUse,
} from "@lit-protocol/agent-keychain-library/lib";
import type { LitRuntime } from "./types.ts";
declare const Lit: LitRuntime;

const METHODS = new Set(["GET", "POST", "PUT", "PATCH", "DELETE"]);
const LABEL = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;
/**
 * Mirrors hostAllowed() in the library's schema.ts (build-only, so not imported
 * here): an exact hostname, or exactly one label under a `*.` entry.
 */
export function hostAllowed(hosts: readonly string[], hostname: string) {
  return hosts.some((h) => {
    if (!h.startsWith("*.")) return h === hostname;
    const suffix = h.slice(1);
    return (
      hostname.endsWith(suffix) &&
      LABEL.test(hostname.slice(0, hostname.length - suffix.length))
    );
  });
}
/**
 * The HTTP client handed to a catalog action. It is the only way an action can
 * reach the network: HTTPS only, hostname allowlist from the manifest (exact, or
 * one label under a `*.` entry), no credentials or ports in the URL, bounded
 * request count, no redirects, and the manifest's timeout and response-size
 * limits. Upstream failures throw.
 */
export function boundFetch(definition: UseDefinition) {
  let requests = 0;
  return async (url: string, init: ActionRequestInit = {}) => {
    const target = new URL(String(url));
    requireThat(
      target.protocol === "https:" &&
        hostAllowed(definition.allowedHosts, target.hostname) &&
        target.username === "" &&
        target.password === "" &&
        target.port === "",
    );
    const method = init.method ?? "GET";
    requireThat(METHODS.has(method));
    requireThat(init.body === undefined || typeof init.body === "string");
    requireThat(++requests <= definition.limits.maxRequests);
    return textFetch(
      target.href,
      { method, headers: init.headers ?? {}, body: init.body },
      definition.limits.timeoutMs,
      definition.limits.maxResponseBytes,
    );
  };
}
/** Runs an action against a decrypted credential and projects its result onto the manifest's output shape. */
export async function useCredential(
  definition: UseDefinition,
  use: ActionUse,
  credential: string,
  input: unknown,
): Promise<Uint8Array<ArrayBuffer>> {
  requireThat(new RegExp(definition.credentialPattern).test(credential));
  const fetchText = boundFetch(definition);
  const result = await use({
    credential,
    input,
    fetchText,
    fetchJson: async (url, init) => JSON.parse(await fetchText(url, init)),
  });
  const output = utf8(
    JSON.stringify(shapeToZod(definition.output).parse(result)),
  );
  requireThat(output.byteLength <= MAX_OUTPUT_BYTES);
  return output;
}
export async function execute(
  raw: Manifest,
  params: any,
  definition: ActionDefinition,
  use?: ActionUse,
) {
  let actionKey: Uint8Array<ArrayBuffer> | undefined;
  let key: Uint8Array<ArrayBuffer> | undefined;
  let plaintext: Uint8Array<ArrayBuffer> | undefined;
  let output: Uint8Array<ArrayBuffer> | undefined;
  try {
    const manifest = manifestSchema.parse(raw);
    requireThat(manifest.release === definition.id);
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
    const operation = definition.operation;
    requireThat(params?.operation === operation);
    const signedRequest = signedRequestSchema.parse(params.signedRequest);
    const request = signedRequest.request;
    const now = nowSeconds();
    requireThat(
      request.operation === operation &&
        request.secretId === manifest.secretId &&
        request.vaultId === manifest.vaultId,
    );
    // Validate agent input against the manifest before any key material is touched.
    let input: unknown;
    if (definition.kind === "use" && definition.input) {
      input = shapeToZod(definition.input).parse(request.input ?? {});
    } else {
      requireThat(request.input === undefined);
    }
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
    // The owner chooses the policy lifetime, including none (expiresAt: null).
    requireThat(
      p.notBefore <= now &&
        (p.expiresAt === null ||
          (p.expiresAt > now && p.expiresAt > p.notBefore)),
    );
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
    const live = () =>
      nowSeconds() < request.expiresAt &&
      (p.expiresAt === null || nowSeconds() < p.expiresAt);
    requireThat(live());
    key = encryptionKey(actionKey);
    plaintext = await decryptEnvelope(envelope, key);
    requireThat(live());
    if (definition.kind === "export") {
      output = plaintext;
    } else {
      requireThat(typeof use === "function");
      output = await useCredential(definition, use, decode(plaintext), input);
    }
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
