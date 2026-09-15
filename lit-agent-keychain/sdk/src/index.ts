import { actionCid, actionSource } from "../../protocol/actions.ts";
import discoverySource from "../../generated/discovery.ts";
import catalog from "../../generated/catalog.ts";
import {
  shapeToZod,
  shapeToJsonSchema,
  type ActionDefinition,
  type Catalog,
  type Shape,
} from "@lit-protocol/agent-keychain-library/schema";
export { shapeToJsonSchema };
import {
  authoritySchema,
  manifestSchema,
  manifestDocumentSchema,
  envelopeSchema,
  policySchema,
  credentialsSchema,
  signedSchema,
  receiptSchema,
  requestSchema,
  V,
  DOMAIN,
  type Authority,
  type OwnerProof,
  type Document,
  type Challenge,
  type Manifest,
  type Signed,
  type Envelope,
  type Policy,
  type Grant,
  type Credentials,
  type KeyBinding,
  type ProtectedResponse,
} from "../../protocol/schema.ts";
import {
  agentPublicKey,
  digest,
  randomId,
  randomBytes,
  unhex,
  hex,
  requireThat,
  nowSeconds,
  verifyReceipt,
  verifyAction,
  signAgent,
  encryptionPublicKey,
  encryptEnvelope,
  open,
  decode,
  responseContext,
} from "../../protocol/crypto.ts";
import { jsonFetch } from "../../protocol/http.ts";
import {
  ATTESTED_ORIGINS,
  verifyAttestation,
  type AttestationPolicy,
  type AttestationReport,
} from "../../protocol/attestation.ts";
export {
  verifyAttestation,
  verifyQuote,
  replayEventLog,
  CHIPOTLE_ATTESTATION_POLICY,
  ATTESTED_ORIGINS,
} from "../../protocol/attestation.ts";
export type {
  AttestationPolicy,
  AttestationReport,
} from "../../protocol/attestation.ts";
export { authorizationTypedData } from "../../protocol/identity.ts";
export {
  actionCid,
  actionSource,
  digest,
  agentPublicKey,
  randomBytes,
  hex,
  unhex,
};
export type { ActionDefinition, Catalog, Shape };
export type {
  Authority,
  Manifest,
  OwnerProof,
  Challenge,
  Document,
  Signed,
  Envelope,
  Policy,
  Grant,
  Credentials,
};
export const DEFAULT_LIT_API_URL = "https://api.chipotle.litprotocol.com";
/**
 * The action catalog compiled into this client: every release id an owner can
 * choose for a secret, with the single operation it permits and, for "use inside
 * Lit" actions, the agent input and result shapes the enclave enforces.
 */
export const ACTIONS: Catalog = catalog;
export function actionDefinition(release: string): ActionDefinition {
  const definition = ACTIONS[release];
  requireThat(definition, `Unknown action release ${release}`);
  return definition;
}
/** Catalog actions owners may pick for a new secret (not deprecated). */
export const availableActions = (tier?: ActionDefinition["tier"]) =>
  Object.values(ACTIONS).filter(
    (d) => !d.deprecated && (tier === undefined || d.tier === tier),
  );
export type OwnerSigner = (challenge: Challenge) => Promise<OwnerProof>;
export type SecretBundle = {
  manifest: Signed<Extract<Document, { kind: "manifest" }>>;
  envelope: Signed<Envelope>;
  policy: Signed<Policy>;
};
export type AgentConfig = {
  v: 2;
  litApiUrl: string;
  usageApiKey?: string;
  secrets: Record<string, { manifest: Manifest; actionCid: string }>;
};
export type AgentIdentity = {
  v?: number;
  privateKey: string;
  publicKey?: string;
};
const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
/**
 * Classifies a value an agent may have been handed. Chipotle usage keys are
 * opaque strings minted by Lit, so this recognizes the Keychain-controlled
 * shapes exactly and treats everything else as an opaque value.
 */
export function describeCredential(
  value: unknown,
):
  | "agent-identity"
  | "agent-private-key"
  | "agent-config"
  | "usage-api-key"
  | "unknown" {
  if (isRecord(value)) {
    if (typeof value.privateKey === "string") return "agent-identity";
    if (value.v === V && isRecord(value.secrets) && "litApiUrl" in value)
      return "agent-config";
    return "unknown";
  }
  if (typeof value !== "string") return "unknown";
  const s = value.trim();
  if (/^(0x)?[0-9a-fA-F]{64}$/.test(s)) return "agent-private-key";
  if (s.startsWith("{")) return "unknown";
  // Chipotle usage keys are currently base64 of 32 random bytes (44 chars).
  if (/^[A-Za-z0-9+/]{43}=$/.test(s)) return "usage-api-key";
  return "unknown";
}
/** Validates the usage key override without pinning Chipotle's exact format. */
export function assertUsageApiKey(value: unknown): asserts value is string {
  requireThat(
    typeof value === "string" && value.length > 0 && value.length <= 512,
    "A scoped Chipotle usage key is required",
  );
  const shape = describeCredential(value);
  requireThat(
    shape !== "agent-private-key",
    "The usage key looks like an agent private key. Pass the usageApiKey from the agent config, never the identity private key.",
  );
  requireThat(
    !value.trim().startsWith("{"),
    "The usage key looks like a JSON file. Pass only the usageApiKey string from the agent config.",
  );
}
/** Validates the identity file written by `keychain init`. */
export function assertAgentIdentity(
  identity: unknown,
): asserts identity is AgentIdentity {
  requireThat(
    isRecord(identity),
    "Agent identity must be the JSON file created by `keychain init`",
  );
  requireThat(
    describeCredential(identity) !== "agent-config",
    "This is an agent config (*.keychain.json), not an agent identity. Check the argument order.",
  );
  requireThat(
    typeof identity.privateKey === "string" &&
      /^[0-9a-f]{64}$/.test(identity.privateKey),
    "Agent identity privateKey must be 64 lowercase hex characters from `keychain init`",
  );
}
/** Validates the agent config downloaded from Keychain (*.keychain.json). */
export function assertAgentConfig(
  config: unknown,
): asserts config is AgentConfig {
  requireThat(
    isRecord(config),
    "Agent config must be the *.keychain.json file downloaded from Keychain",
  );
  requireThat(
    describeCredential(config) !== "agent-identity",
    "This is an agent identity, not an agent config (*.keychain.json). Check the argument order.",
  );
  requireThat(config.v === V, "Unsupported agent config version");
  requireThat(
    typeof config.litApiUrl === "string",
    "Agent config is missing litApiUrl",
  );
  requireThat(
    isRecord(config.secrets) &&
      Object.values(config.secrets).every(
        (s) =>
          isRecord(s) &&
          isRecord(s.manifest) &&
          typeof s.actionCid === "string",
      ),
    "Agent config secrets must map names to { manifest, actionCid }",
  );
  if (config.usageApiKey !== undefined) assertUsageApiKey(config.usageApiKey);
}
const post = (body: unknown): RequestInit => ({
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(body),
});
function origin(value: string) {
  const u = new URL(value);
  requireThat(
    u.origin === value &&
      (u.protocol === "https:" ||
        (u.protocol === "http:" &&
          ["127.0.0.1", "localhost", "[::1]"].includes(u.hostname))),
    "Invalid origin",
  );
  return value;
}
/**
 * How a connection attests its Lit endpoint before sending anything.
 * `false` disables the check; a policy pins the expected dstack app and
 * governance contracts. Omitted: known Lit origins use their pinned policy and
 * unknown origins (local development, test adapters) are not attested.
 */
export type AttestationOption = AttestationPolicy | false | undefined;
export type AttestationHooks = {
  /** SHA-256 of the DER TLS certificate observed for the endpoint (Node only). */
  tlsCertificateSha256?: string;
  /** Re-verify after this many milliseconds. Default one hour. */
  maxAgeMs?: number;
};
export class LitConnection {
  readonly url: string;
  private readonly keys = new Map<string, string>();
  readonly attestationPolicy: AttestationPolicy | undefined;
  private attestationHooks: AttestationHooks = {};
  private attested: Promise<AttestationReport> | undefined;
  private attestedAt = 0;
  constructor(
    url = DEFAULT_LIT_API_URL,
    readonly timeoutMs = 30000,
    public usageApiKey?: string,
    attestation: AttestationOption = undefined,
    hooks: AttestationHooks = {},
  ) {
    this.url = origin(url);
    this.attestationPolicy =
      attestation === false
        ? undefined
        : (attestation ?? ATTESTED_ORIGINS[this.url]);
    this.attestationHooks = hooks;
  }
  /**
   * Proves the endpoint is a genuine, governed Lit TEE before any request. The
   * result is cached per connection and refreshed after `maxAgeMs`. Throws (and
   * leaves the connection unusable for that call) if any check fails.
   */
  async attest(): Promise<AttestationReport | undefined> {
    if (!this.attestationPolicy) return undefined;
    const maxAge = this.attestationHooks.maxAgeMs ?? 3600000;
    if (!this.attested || Date.now() - this.attestedAt > maxAge) {
      this.attestedAt = Date.now();
      this.attested = verifyAttestation(this.url, this.attestationPolicy, {
        timeoutMs: this.timeoutMs,
        tlsCertificateSha256: this.attestationHooks.tlsCertificateSha256,
      }).catch((error) => {
        this.attested = undefined;
        throw error;
      });
    }
    return this.attested;
  }
  async publicKey(cid: string) {
    const cached = this.keys.get(cid);
    if (cached) return cached;
    // Deliberately direct to the caller-configured trusted Lit endpoint. Never
    // take a replacement endpoint/key from Keychain's API or a secret bundle.
    const result = await this.direct(discoverySource, { cid });
    requireThat(
      typeof result.public_key === "string" &&
        /^(0x)?(?:02|03)[0-9a-f]{64}$|^(0x)?04[0-9a-f]{128}$/.test(
          result.public_key,
        ),
    );
    if (this.keys.size >= 2048)
      this.keys.delete(this.keys.keys().next().value!);
    this.keys.set(cid, result.public_key);
    return result.public_key;
  }
  private async direct(code: string, params: unknown) {
    requireThat(
      typeof this.usageApiKey === "string" &&
        this.usageApiKey.length > 0 &&
        this.usageApiKey.length <= 512,
      "A scoped Chipotle usage key is required",
    );
    await this.attest();
    const result = await jsonFetch(
      `${this.url}/core/v1/lit_action`,
      {
        ...post({ code, js_params: params }),
        headers: {
          "Content-Type": "application/json",
          "X-Api-Key": this.usageApiKey,
          "X-Privacy-Mode": "true",
        },
      },
      this.timeoutMs,
    );
    requireThat(result.has_error === false, "Lit execution failed");
    requireThat(
      result.response?.ok === true,
      result.response?.error === "authorization_denied"
        ? "Owner authorization denied"
        : "Access denied",
    );
    return result.response;
  }
  async execute(manifest: Authority | Manifest, params: unknown) {
    if (this.usageApiKey) return this.direct(actionSource(manifest), params);
    requireThat(
      "owner" in manifest && (params as any)?.document?.kind === "login",
      "Sign in before executing actions",
    );
    const kind = "owner" in manifest ? "authority" : "secret";
    const expected = await actionCid(manifest);
    const result = await jsonFetch(
      `${manifest.registry}/api/execute`,
      post({ kind, manifest, params }),
      this.timeoutMs,
    );
    requireThat(result.actionCid === expected, "Action identity mismatch");
    requireThat(
      result.response?.ok === true,
      result.response?.error === "authorization_denied"
        ? "Owner authorization denied"
        : "Access denied",
    );
    return result.response;
  }
  async encryptionPublicKey(manifest: Manifest) {
    const challenge = randomId();
    const cid = await actionCid(manifest);
    const [root, result] = await Promise.all([
      this.publicKey(cid),
      this.execute(manifest, { operation: "publicKey", challenge }),
    ]);
    const binding = result.binding as KeyBinding;
    requireThat(
      binding?.payload?.v === V &&
        binding.payload.domain === "lit-keychain/key-binding/v2" &&
        binding.payload.manifestHash === digest(manifest) &&
        binding.payload.challenge === challenge &&
        /^[0-9a-f]{64}$/.test(binding.payload.encryptionPublicKey),
      "Invalid public-key binding",
    );
    verifyAction(binding.payload, binding.signature, root);
    return binding.payload.encryptionPublicKey;
  }
}
export class OwnerClient {
  readonly authority: Authority;
  readonly vaultId: string;
  readonly lit: LitConnection;
  /**
   * Optional progress reporter for long-running owner flows. `login()` calls it
   * with a short human-readable status before each step. First sign-in provisions
   * the vault's Chipotle groups and execution key, which are on-chain writes and
   * take roughly 30 seconds.
   */
  progress?: (message: string) => void;
  constructor(
    authority: Authority,
    readonly signer: OwnerSigner,
    lit = new LitConnection(),
    readonly managementTimeoutMs = 120000,
  ) {
    this.authority = authoritySchema.parse(authority);
    this.vaultId = digest(this.authority);
    this.lit = lit;
  }
  async api(path: string, init: RequestInit = {}) {
    return jsonFetch(
      this.authority.registry + path,
      { ...init, credentials: "include" },
      this.managementTimeoutMs,
    );
  }
  async authorize<T extends Document>(document: T): Promise<Signed<T>> {
    if (!this.lit.usageApiKey) await this.login();
    const signed = await this.approve(document);
    verifyReceipt(
      signed,
      await this.lit.publicKey(await actionCid(this.authority)),
      this.vaultId,
    );
    return signed;
  }
  private async approve<T extends Document>(document: T): Promise<Signed<T>> {
    requireThat(document.vaultId === this.vaultId);
    const now = nowSeconds();
    const challenge: Challenge = {
      v: V,
      domain: "lit-keychain/authorize/v2",
      vaultId: this.vaultId,
      objectHash: digest(document),
      operation: document.kind,
      nonce: randomId(),
      issuedAt: now,
      expiresAt: now + 120,
    };
    const proof = await this.signer(challenge);
    const response = await this.lit.execute(this.authority, {
      document,
      proof,
    });
    return { document, receipt: receiptSchema.parse(response.receipt) };
  }
  async login() {
    this.progress?.("Requesting a sign-in challenge…");
    const document = await this.api("/auth/challenge", post(this.authority));
    requireThat(document.kind === "login" && document.vaultId === this.vaultId);
    this.progress?.("Verifying owner authorization…");
    const authorization = await this.approve(document);
    const result = await this.api(
      "/auth/login",
      post({ authority: this.authority, authorization }),
    );
    this.progress?.(
      "Preparing your vault's execution key on the Lit network… " +
        "The first sign-in takes about 30 seconds.",
    );
    const { usageApiKey } = await this.api("/api/execution-key", {
      method: "POST",
    });
    this.lit.usageApiKey = usageApiKey;
    try {
      // The bootstrap proxy establishes only an app session. Authenticate its
      // receipt over a direct Chipotle connection before trusting it or importing.
      this.progress?.("Verifying the Lit endpoint and sign-in receipt…");
      verifyReceipt(
        authorization,
        await this.lit.publicKey(await actionCid(this.authority)),
        this.vaultId,
      );
    } catch (error) {
      this.lit.usageApiKey = undefined;
      await this.api("/auth/logout", { method: "POST" }).catch(() => {});
      throw error;
    }
    return result;
  }
  async create(
    name: string,
    plaintext: string,
    release: Manifest["release"] = "export",
  ) {
    const definition = actionDefinition(release);
    requireThat(!definition.deprecated, "This action release is deprecated");
    if (definition.kind === "use")
      requireThat(
        new RegExp(definition.credentialPattern).test(plaintext),
        `The value does not look like a credential for ${definition.name}`,
      );
    const manifest: Manifest = {
      v: V,
      network: this.authority.network,
      registry: this.authority.registry,
      vaultId: this.vaultId,
      authorityCid: await actionCid(this.authority),
      secretId: randomId(),
      release,
    };
    const cid = await actionCid(manifest);
    const signedManifest = await this.authorize({
      v: V,
      domain: DOMAIN,
      kind: "manifest",
      vaultId: this.vaultId,
      manifest,
      actionCid: cid,
    });
    await this.api("/api/actions", post(signedManifest));
    const key = await this.lit.encryptionPublicKey(manifest);
    const envelope = await encryptEnvelope(
      {
        v: V,
        domain: DOMAIN,
        vaultId: this.vaultId,
        secretId: manifest.secretId,
        actionCid: cid,
        version: 1,
        release,
        name,
      },
      key,
      plaintext,
    );
    const signedEnvelope = await this.authorize(envelope);
    const now = nowSeconds();
    const policy = await this.authorize({
      v: V,
      domain: DOMAIN,
      kind: "policy",
      vaultId: this.vaultId,
      secretId: manifest.secretId,
      actionCid: cid,
      epoch: 1,
      previousHash: null,
      disabled: false,
      notBefore: now,
      expiresAt: now + 30 * 86400,
      grants: [],
    });
    const bundle: SecretBundle = {
      manifest: signedManifest,
      envelope: signedEnvelope,
      policy,
    };
    await this.api("/api/secrets", post(bundle));
    return bundle;
  }
  async bundle(secretId: string): Promise<SecretBundle> {
    requireThat(/^[0-9a-f]{64}$/.test(secretId));
    const bundle = await this.api(`/api/secrets/${secretId}/bundle`);
    await verifyBundle(bundle, this.lit, this.vaultId);
    requireThat(bundle.manifest.document.manifest.secretId === secretId);
    requireThat(
      bundle.manifest.document.manifest.authorityCid ===
        (await actionCid(this.authority)),
    );
    return bundle;
  }
  async setPolicy(
    bundle: SecretBundle,
    changes: { grants?: Grant[]; disabled?: boolean; days?: number },
  ) {
    const old = bundle.policy.document;
    const now = nowSeconds();
    const days = changes.days ?? 30;
    requireThat(Number.isInteger(days) && days >= 1 && days <= 90);
    const policy = policySchema.parse({
      ...old,
      epoch: old.epoch + 1,
      previousHash: digest(old),
      notBefore: now,
      expiresAt: now + days * 86400,
      grants: changes.grants ?? old.grants,
      disabled: changes.disabled ?? old.disabled,
    });
    const signed = await this.authorize(policy);
    await this.api(`/api/secrets/${old.secretId}/policy`, {
      ...post(signed),
      method: "PUT",
    });
    return { ...bundle, policy: signed };
  }
  async delegate(bundle: SecretBundle, publicKey: string, label: string) {
    const grant: Grant = {
      agentPublicKey: publicKey,
      label,
      operations: [
        actionDefinition(bundle.manifest.document.manifest.release).operation,
      ],
      versions: [
        {
          version: bundle.envelope.document.metadata.version,
          envelopeHash: digest(bundle.envelope.document),
        },
      ],
    };
    return this.setPolicy(bundle, {
      grants: [
        ...bundle.policy.document.grants.filter(
          (g) => g.agentPublicKey !== publicKey,
        ),
        grant,
      ],
    });
  }
  async rotate(bundle: SecretBundle, plaintext: string) {
    const manifest = bundle.manifest.document.manifest;
    const envelope = await encryptEnvelope(
      {
        ...bundle.envelope.document.metadata,
        version: bundle.envelope.document.metadata.version + 1,
      },
      await this.lit.encryptionPublicKey(manifest),
      plaintext,
    );
    const signedEnvelope = await this.authorize(envelope);
    const now = nowSeconds();
    const policy = await this.authorize({
      ...bundle.policy.document,
      epoch: bundle.policy.document.epoch + 1,
      previousHash: digest(bundle.policy.document),
      notBefore: now,
      expiresAt: now + 30 * 86400,
      grants: bundle.policy.document.grants.map((g) => ({
        ...g,
        versions: [
          {
            version: envelope.metadata.version,
            envelopeHash: digest(envelope),
          },
        ],
      })),
    });
    const updated = {
      manifest: bundle.manifest,
      envelope: signedEnvelope,
      policy,
    };
    await this.api(`/api/secrets/${manifest.secretId}/rotate`, post(updated));
    return updated;
  }
  async getCredentials(): Promise<Signed<Credentials> | null> {
    const state = await this.api(`/api/registry/credentials/${this.vaultId}`);
    requireThat(Object.hasOwn(state, "policy"));
    if (state.policy === null) return null;
    const signed = signedSchema(credentialsSchema).parse(state.policy);
    verifyReceipt(
      signed,
      await this.lit.publicKey(await actionCid(this.authority)),
      this.vaultId,
    );
    return signed;
  }
  async updateCredentials(owners: Authority["owner"][]) {
    const previous = (await this.getCredentials())?.document;
    const now = nowSeconds();
    const signed = await this.authorize({
      v: V,
      domain: DOMAIN,
      kind: "credentials",
      vaultId: this.vaultId,
      epoch: (previous?.epoch ?? 0) + 1,
      previousHash: previous ? digest(previous) : null,
      owners,
      notBefore: now,
      expiresAt: null,
    });
    await this.api("/api/credentials", { ...post(signed), method: "PUT" });
  }
  async backup() {
    const secrets = await this.listSecrets();
    const bundles = [];
    for (const secret of secrets)
      bundles.push(await this.bundle(secret.secretId));
    return {
      v: V,
      authority: this.authority,
      credentials: await this.getCredentials(),
      bundles,
    };
  }
  async listSecrets() {
    const secrets: any[] = [];
    let after = "";
    for (;;) {
      const page = await this.api(
        "/api/secrets" + (after ? `?after=${after}` : ""),
      );
      requireThat(
        Array.isArray(page.secrets) &&
          page.secrets.length <= 200 &&
          secrets.length + page.secrets.length <= 100000,
        "Invalid secret listing",
      );
      secrets.push(...page.secrets);
      if (page.nextCursor === null) return secrets;
      requireThat(
        typeof page.nextCursor === "string" &&
          /^[0-9a-f]{64}$/.test(page.nextCursor) &&
          page.nextCursor > after,
        "Invalid listing cursor",
      );
      after = page.nextCursor;
    }
  }
  static async restoreCredentials(
    backup: { authority: Authority; credentials?: Signed<Credentials> | null },
    lit = new LitConnection(),
  ) {
    const authority = authoritySchema.parse(backup.authority);
    if (!backup.credentials) return;
    const credentials = signedSchema(credentialsSchema).parse(
      backup.credentials,
    );
    requireThat(credentials.document.vaultId === digest(authority));
    // A fresh device may have no usage key before recovery/sign-in. Upload only
    // the signed public backup; the API and immutable authority action verify it.
    // Sign-in then bootstraps direct Chipotle verification before any secret use.
    if (lit.usageApiKey)
      verifyReceipt(
        credentials,
        await lit.publicKey(await actionCid(authority)),
        digest(authority),
      );
    await jsonFetch(
      authority.registry + "/auth/restore-credentials",
      post({ authority, credentials }),
      lit.timeoutMs,
    );
  }
  async restore(backup: {
    v: number;
    authority: Authority;
    bundles: SecretBundle[];
  }) {
    requireThat(
      backup.v === V &&
        digest(backup.authority) === this.vaultId &&
        Array.isArray(backup.bundles) &&
        backup.bundles.length <= 100000,
    );
    for (const bundle of backup.bundles) {
      await verifyBundle(bundle, this.lit, this.vaultId);
      requireThat(
        bundle.manifest.document.manifest.authorityCid ===
          (await actionCid(this.authority)),
      );
      await this.api("/api/actions", post(bundle.manifest));
      await this.api("/api/restore", post(bundle));
    }
  }
}
export async function verifyBundle(
  bundle: SecretBundle,
  lit: LitConnection,
  vaultId?: string,
) {
  const signedManifest = signedSchema(manifestDocumentSchema).parse(
    bundle.manifest,
  );
  const m = signedManifest.document.manifest;
  requireThat(signedManifest.document.vaultId === m.vaultId);
  requireThat(!vaultId || m.vaultId === vaultId);
  const cid = await actionCid(m);
  requireThat(bundle.manifest.document.actionCid === cid);
  const key = await lit.publicKey(m.authorityCid);
  const env = signedSchema(envelopeSchema).parse(bundle.envelope);
  const policy = signedSchema(policySchema).parse(bundle.policy);
  for (const signed of [signedManifest, env, policy])
    verifyReceipt<unknown>(signed, key, m.vaultId);
  requireThat(
    env.document.metadata.secretId === m.secretId &&
      env.document.metadata.actionCid === cid &&
      env.document.metadata.vaultId === m.vaultId &&
      env.document.metadata.release === m.release &&
      policy.document.secretId === m.secretId &&
      policy.document.actionCid === cid &&
      policy.document.vaultId === m.vaultId,
  );
  return m;
}
export class Keychain {
  private readonly key: Uint8Array<ArrayBuffer>;
  readonly publicKey: string;
  readonly lit: LitConnection;
  constructor(
    privateKey: string,
    readonly config: AgentConfig,
    options: {
      timeoutMs?: number;
      usageApiKey?: string;
      attestation?: AttestationOption;
      tlsCertificateSha256?: string;
    } = {},
  ) {
    assertAgentIdentity({ privateKey });
    assertAgentConfig(config);
    if (options.usageApiKey !== undefined)
      assertUsageApiKey(options.usageApiKey);
    this.key = unhex(privateKey);
    this.publicKey = agentPublicKey(this.key);
    this.lit = new LitConnection(
      config.litApiUrl,
      options.timeoutMs,
      options.usageApiKey ?? config.usageApiKey,
      options.attestation,
      { tlsCertificateSha256: options.tlsCertificateSha256 },
    );
  }
  /** Attests the Lit endpoint now instead of lazily on the first read. */
  attest() {
    return this.lit.attest();
  }
  /** Secret names with the single operation each release permits and its input shape, if any. */
  list(): {
    name: string;
    release: Manifest["release"];
    operation: string;
    input?: Shape | null;
  }[] {
    return Object.entries(this.config.secrets).map(([name, locator]) => {
      const definition = ACTIONS[locator.manifest.release];
      return {
        name,
        release: locator.manifest.release,
        operation: definition?.operation ?? "unknown",
        ...(definition?.kind === "use" ? { input: definition.input } : {}),
      };
    });
  }
  static generateKey() {
    const key = randomBytes();
    return { privateKey: hex(key), publicKey: agentPublicKey(key) };
  }
  destroy() {
    this.key.fill(0);
  }
  /** Decrypts an export-release secret locally and returns its value. */
  async get(name: string): Promise<string> {
    return this.read(name, "get");
  }
  /**
   * Runs a "use inside Lit" catalog action with the credential, never revealing it.
   * `input` is validated here against the action's declared shape before signing,
   * and again inside the enclave. Returns the action's bounded result object.
   */
  async use(name: string, input?: Record<string, unknown>): Promise<any> {
    const locator = this.config.secrets[name];
    requireThat(locator, "Unknown secret");
    const definition = actionDefinition(locator.manifest.release);
    requireThat(
      definition.kind === "use",
      "Secret is an export release; call get()",
    );
    let validated: Record<string, unknown> | undefined;
    if (definition.input) {
      const parsed = shapeToZod(definition.input).safeParse(input ?? {});
      requireThat(
        parsed.success,
        `Invalid input for ${definition.name}: ${parsed.success ? "" : parsed.error.issues.map((i) => `${i.path.join(".") || "input"} ${i.message}`).join("; ")}`,
      );
      validated = parsed.data as Record<string, unknown>;
    } else {
      requireThat(
        input === undefined || Object.keys(input).length === 0,
        `${definition.name} takes no input`,
      );
    }
    return JSON.parse(await this.read(name, definition.operation, validated));
  }
  async stripeBalance(name: string) {
    return this.use(name);
  }
  private async read(
    name: string,
    operation: string,
    input?: Record<string, unknown>,
  ): Promise<string> {
    const locator = this.config.secrets[name];
    requireThat(locator, "Unknown secret");
    const manifest = manifestSchema.parse(locator.manifest);
    requireThat(
      (await actionCid(manifest)) === locator.actionCid,
      "Pinned action CID mismatch",
    );
    requireThat(
      actionDefinition(manifest.release).operation === operation,
      "Unsupported release operation",
    );
    const bundle: SecretBundle = await jsonFetch(
      `${manifest.registry}/api/secrets/${manifest.secretId}/bundle`,
      {},
      this.lit.timeoutMs,
    );
    await verifyBundle(bundle, this.lit, manifest.vaultId);
    requireThat(
      digest(bundle.manifest.document.manifest) === digest(manifest),
      "Manifest substitution",
    );
    const now = nowSeconds();
    const responseKey = randomBytes();
    const request = requestSchema.parse({
      v: V,
      domain: DOMAIN,
      vaultId: manifest.vaultId,
      secretId: manifest.secretId,
      actionCid: locator.actionCid,
      version: bundle.envelope.document.metadata.version,
      envelopeHash: digest(bundle.envelope.document),
      policyHash: digest(bundle.policy.document),
      agentPublicKey: this.publicKey,
      operation,
      ...(input !== undefined ? { input } : {}),
      responsePublicKey: encryptionPublicKey(responseKey),
      nonce: randomId(),
      issuedAt: now,
      expiresAt: now + 90,
    });
    try {
      const result = await this.lit.execute(manifest, {
        operation,
        signedRequest: { request, signature: signAgent(request, this.key) },
        envelope: bundle.envelope,
      });
      const protectedResult = result.result as ProtectedResponse;
      requireThat(
        protectedResult?.payload?.v === V &&
          protectedResult.payload.domain === "lit-keychain/response/v2" &&
          protectedResult.payload.requestHash === digest(request),
      );
      verifyAction(
        protectedResult.payload,
        protectedResult.signature,
        await this.lit.publicKey(locator.actionCid),
      );
      const bytes = await open(
        responseKey,
        protectedResult.payload.sealed,
        responseContext(request),
      );
      try {
        return decode(bytes);
      } finally {
        bytes.fill(0);
      }
    } finally {
      responseKey.fill(0);
    }
  }
}
