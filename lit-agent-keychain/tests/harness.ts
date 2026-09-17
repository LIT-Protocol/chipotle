import vm from "node:vm";
import { webcrypto } from "node:crypto";
import { secp256k1 } from "@noble/curves/secp256k1.js";
import { privateKeyToAccount } from "viem/accounts";
import { actionCid, actionSource } from "../protocol/actions.ts";
import catalog from "../generated/catalog.ts";
import {
  digest,
  unhex,
  hex,
  randomBytes,
  agentPublicKey,
  encryptEnvelope,
  encryptionKey,
  encryptionPublicKey,
  makeReceipt,
  signAgent,
  randomId,
  nowSeconds,
} from "../protocol/crypto.ts";
import { authorizationTypedData } from "../protocol/identity.ts";
import {
  DOMAIN,
  V,
  type Authority,
  type Manifest,
  type Document,
  type Signed,
  type Envelope,
  type Policy,
  type Challenge,
} from "../protocol/schema.ts";
export const keyFor = (cid: string) =>
  unhex(digest({ purpose: "TEST-ONLY-ACTION-KEY", cid }));
export const pubFor = (cid: string) => hex(secp256k1.getPublicKey(keyFor(cid)));
export const json = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
export class Harness {
  registry = new Map<string, any>();
  extraFetch: typeof fetch | undefined;
  calls: { url: string; init?: RequestInit }[] = [];
  privateKeyCalls = 0;
  fetch: typeof fetch = async (input, init) => {
    const url = String(input);
    this.calls.push({ url, init });
    if (url.includes("/api/registry/credentials/"))
      return json({ policy: this.registry.get(url) ?? null });
    if (this.registry.has(url)) return json({ policy: this.registry.get(url) });
    if (this.extraFetch) return this.extraFetch(input, init);
    return json({ error: "not_found" }, 404);
  };
  async run(manifest: Authority | Manifest, params: unknown) {
    const cid = await actionCid(manifest);
    return this.runCode(actionSource(manifest), cid, params);
  }
  async runCode(code: string, cid: string, params: unknown) {
    const context = vm.createContext({
      crypto: webcrypto,
      fetch: this.fetch,
      TextEncoder,
      TextDecoder,
      URL,
      URLSearchParams,
      AbortController,
      Response,
      Request,
      Headers,
      CryptoKey: globalThis.CryptoKey,
      structuredClone,
      setTimeout,
      clearTimeout,
      atob,
      btoa,
      Lit: {
        Actions: {
          getLitActionPrivateKey: async () => {
            this.privateKeyCalls++;
            return "0x" + hex(keyFor(cid));
          },
          getLitActionPublicKey: async ({ ipfsId }: { ipfsId: string }) =>
            "0x" + pubFor(ipfsId),
        },
      },
    });
    if (process.env.KEYCHAIN_TEST_DEBUG)
      code = code.replace(
        'catch{return{ok:!1,error:"access_denied"}}',
        "catch(error){throw error}",
      );
    const result = await vm.runInContext(
      code + "\nmain(" + JSON.stringify(params) + ")",
      context,
      { timeout: 15000 },
    );
    return JSON.parse(JSON.stringify(result));
  }
}
/** Well-formed sample credentials per catalog action (test values, not real keys). */
export const SAMPLE_CREDENTIALS: Record<string, string> = {
  export: "super-secret-☃",
  stripe_balance: "sk_test_abcdefghijklmnopqrstuvwxyz",
  openai_chat: "sk-" + "a".repeat(40),
  github_read_file: "ghp_" + "A".repeat(36),
  slack_post_message: "xoxb-1234567890-abcdefghijkl",
  supabase_tables: JSON.stringify({
    ref: "abcdefghijklmnopqrst",
    key: "sb_secret_" + "k".repeat(40),
    tables: {
      orders: {
        select: ["id", "status", "total_cents"],
        filter: ["id", "status"],
        insert: ["status"],
        maxRows: 50,
      },
    },
  }),
};
export const operationFor = (release: string) => {
  const definition = catalog[release];
  if (!definition) throw new Error(`Unknown release ${release}`);
  return definition.operation;
};
export async function fixture(
  release: Manifest["release"] = "export",
  registry = "https://keychain.test",
  options: { secret?: string; input?: Record<string, unknown> } = {},
) {
  const h = new Harness();
  const ownerKey = randomBytes();
  const account = privateKeyToAccount(`0x${hex(ownerKey)}`);
  const authority: Authority = {
    v: V,
    network: "test",
    registry,
    owner: { kind: "wallet", address: account.address.toLowerCase() },
  };
  const vaultId = digest(authority);
  const authorityCid = await actionCid(authority);
  const manifest: Manifest = {
    v: V,
    network: "test",
    registry: authority.registry,
    vaultId,
    authorityCid,
    secretId: randomId(),
    release,
  };
  const cid = await actionCid(manifest);
  const agentKey = randomBytes();
  const responseKey = randomBytes();
  const secret = options.secret ?? SAMPLE_CREDENTIALS[release];
  if (secret === undefined)
    throw new Error(`No sample credential for ${release}`);
  const operation = operationFor(release);
  const envelope = await encryptEnvelope(
    {
      v: V,
      domain: DOMAIN,
      vaultId,
      secretId: manifest.secretId,
      actionCid: cid,
      version: 1,
      release,
      name: "TEST_SECRET",
    },
    encryptionPublicKey(encryptionKey(keyFor(cid))),
    secret,
  );
  const now = nowSeconds();
  const policy: Policy = {
    v: V,
    domain: DOMAIN,
    vaultId,
    kind: "policy",
    secretId: manifest.secretId,
    actionCid: cid,
    epoch: 1,
    previousHash: null,
    notBefore: now - 1,
    expiresAt: now + 3600,
    disabled: false,
    grants: [
      {
        agentPublicKey: agentPublicKey(agentKey),
        label: "test",
        operations: [operation],
        versions: [{ version: 1, envelopeHash: digest(envelope) }],
      },
    ],
  };
  const sign = <T extends { vaultId: string }>(document: T): Signed<T> => ({
    document,
    receipt: makeReceipt(document, keyFor(authorityCid), now),
  });
  const registryUrl = `${authority.registry}/api/registry/secrets/${manifest.secretId}`;
  h.registry.set(registryUrl, sign(policy));
  const request = {
    v: V,
    domain: DOMAIN,
    vaultId,
    secretId: manifest.secretId,
    actionCid: cid,
    version: 1,
    envelopeHash: digest(envelope),
    policyHash: digest(policy),
    agentPublicKey: agentPublicKey(agentKey),
    operation,
    ...(options.input !== undefined ? { input: options.input } : {}),
    responsePublicKey: encryptionPublicKey(responseKey),
    nonce: randomId(),
    issuedAt: now,
    expiresAt: now + 90,
  };
  const params = {
    operation: request.operation,
    signedRequest: { request, signature: signAgent(request, agentKey) },
    envelope: sign(envelope),
  };
  async function ownerProof(document: { kind: string }): Promise<any> {
    const challenge: Challenge = {
      v: V,
      domain: "lit-keychain/authorize/v2",
      vaultId,
      objectHash: digest(document),
      operation: document.kind as Challenge["operation"],
      nonce: randomId(),
      issuedAt: now,
      expiresAt: now + 120,
    };
    return {
      kind: "wallet",
      owner: authority.owner,
      challenge,
      signature: await account.signTypedData(authorizationTypedData(challenge)),
    };
  }
  return {
    h,
    authority,
    authorityCid,
    manifest,
    cid,
    agentKey,
    responseKey,
    secret,
    envelope,
    policy,
    request,
    params,
    sign,
    registryUrl,
    ownerProof,
    now,
  };
}
