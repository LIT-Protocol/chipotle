import {
  actionCid,
  actionSource,
  cidForCode,
  templateStore,
  type Template,
} from "../../protocol/actions.ts";
import discoverySource from "../../generated/discovery.ts";
import { peerCertificateSha256 } from "./tls-runtime.ts";
import catalog from "../../generated/catalog.ts";
import {
  shapeToZod,
  shapeToJsonSchema,
  type ActionDefinition,
  type Catalog,
  type Shape,
} from "@lit-protocol/agent-keychain-library/schema";
export { shapeToJsonSchema };
export {
  LiveKeychain,
  type LiveKeychainOptions,
  type LiveSecretInfo,
} from "./live.ts";
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
  type Batch,
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
import { jsonFetch, HttpError } from "../../protocol/client-http.ts";
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
  BASE_PUBLIC_RPC_URLS,
} from "../../protocol/attestation.ts";
export type {
  AttestationPolicy,
  AttestationReport,
} from "../../protocol/attestation.ts";
export { authorizationTypedData } from "../../protocol/identity.ts";
export { HttpError } from "../../protocol/client-http.ts";
export {
  actionCid,
  actionSource,
  templateStore,
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
export const DEFAULT_KEYCHAIN_SERVICE_URL = "https://keychain.litprotocol.com";
/**
 * The action catalog compiled into this client: every release id an owner can
 * choose for a secret, with the single operation it permits and, for "use inside
 * Lit" actions, the agent input and result shapes the enclave enforces.
 */
export const ACTIONS: Catalog = catalog;
export function actionDefinition(release: string): ActionDefinition {
  const definition = Object.hasOwn(ACTIONS, release)
    ? ACTIONS[release]
    : undefined;
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
  if (s.startsWith("{")) {
    // The raw text of an identity or config file, as an agent that just read
    // the file from disk would hold it.
    try {
      const parsed: unknown = JSON.parse(s);
      return isRecord(parsed) ? describeCredential(parsed) : "unknown";
    } catch {
      return "unknown";
    }
  }
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
  requireThat(
    !Object.hasOwn(identity, "v") || identity.v === V,
    "Unsupported agent identity version",
  );
  if (Object.hasOwn(identity, "publicKey")) {
    const key = unhex(identity.privateKey);
    try {
      requireThat(
        identity.publicKey === agentPublicKey(key),
        "Agent identity publicKey does not match privateKey",
      );
    } finally {
      key.fill(0);
    }
  }
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
/** Retry policy for the first request made with a freshly minted usage key. */
export type FreshKeySettle = {
  /** Give up after this many milliseconds. Default 45000. */
  ms?: number;
  /** Base backoff step in milliseconds; doubles per round, capped at 5 steps. */
  stepMs?: number;
  /** Called before each wait with the 1-based retry count. */
  onWait?: (retry: number) => void;
};
/**
 * Authority releases published before batched approval existed. Secrets pinned
 * to one of these are still rotated under it, one signature per document.
 * Append-only: a release's bytes never change, so this list never shrinks.
 */
export const PRE_BATCH_AUTHORITY_HASHES: ReadonlySet<string> = new Set([
  "29314b8876a199afde0396711530c4de9de25eca7f99202a09a4d68f32cd0a66",
  "1c042eed5e526f747a7533cedb0f3b02dff8ad05143d07299717193b4f6e8e2b",
]);
/**
 * Authority releases whose bytes still enforce a 90-day policy lifetime. Secrets
 * pinned to one of them keep that cap until they are recreated under a newer
 * release; later releases leave the lifetime to the owner. Append-only.
 */
export const CAPPED_POLICY_AUTHORITY_HASHES: ReadonlySet<string> = new Set([
  ...PRE_BATCH_AUTHORITY_HASHES,
  "4ca83f7cd984356d56c8bdd455f8ee358cb3dfaa423d7693b34212a0c1b0f3e6",
]);
const LEGACY_POLICY_CAP_DAYS = 90;
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
      this.attested = (async () => {
        const tlsCertificateSha256 =
          this.attestationHooks.tlsCertificateSha256 ??
          (await peerCertificateSha256(this.url, this.timeoutMs));
        return verifyAttestation(this.url, this.attestationPolicy!, {
          timeoutMs: this.timeoutMs,
          tlsCertificateSha256,
        });
      })().catch((error) => {
        this.attested = undefined;
        throw error;
      });
    }
    return this.attested;
  }
  async publicKey(cid: string, settle?: FreshKeySettle) {
    const cached = this.keys.get(cid);
    if (cached) return cached;
    // Deliberately direct to the caller-configured trusted Lit endpoint. Never
    // take a replacement endpoint/key from Keychain's API or a secret bundle.
    const result = await this.settled(
      () => this.direct(discoverySource, { cid }),
      settle,
    );
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
  /**
   * Runs `attempt`, retrying while Chipotle answers 401/403. A usage key that was
   * minted moments ago is an on-chain write; Chipotle's authorization reads go
   * through load-balanced RPC backends and one may not have imported that block
   * yet, and a denial is cached there for ~30 seconds. Bounded by `settle.ms`
   * (default 45 seconds) so a genuinely unauthorized key still fails.
   */
  private async settled<T>(
    attempt: () => Promise<T>,
    settle: FreshKeySettle | undefined,
  ): Promise<T> {
    if (!settle) return attempt();
    const deadline = Date.now() + (settle.ms ?? 45000);
    const step = settle.stepMs ?? 1000;
    for (let round = 0; ; round++) {
      try {
        return await attempt();
      } catch (error) {
        const denied =
          error instanceof HttpError &&
          (error.status === 401 || error.status === 403);
        const wait = Math.min(step * 2 ** round, 5 * step);
        if (!denied || Date.now() + wait > deadline) throw error;
        settle.onWait?.(round + 1);
        await new Promise((r) => setTimeout(r, wait));
      }
    }
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
    ).catch((error: unknown) => {
      // Chipotle answers 401 when the scoped execution key no longer resolves,
      // which for an agent almost always means the owner replaced it. Keep the
      // HttpError type and status: login() retries 401/403 for fresh keys.
      if (error instanceof HttpError && error.status === 401)
        throw new HttpError(
          401,
          `${(error.detail || "execution key rejected").replace(/[.\s]+$/, "")}. The scoped execution key in this agent config is not accepted by Lit; ask the owner to download a fresh agent config using Download agent config next to the agent under Authorized agents. If a fresh config is still rejected, contact Keychain support. Advanced clients may override CHIPOTLE_USAGE_API_KEY`,
        );
      throw error;
    });
    requireThat(result.has_error === false, "Lit execution failed");
    requireThat(
      result.response?.ok === true,
      result.response?.error === "authorization_denied"
        ? "Owner authorization denied"
        : "Access denied",
    );
    return result.response;
  }
  /**
   * Runs the action for `manifest`. `template` selects an earlier release of its
   * template (exact bytes, verified by hash); by default the current release runs.
   */
  async execute(
    manifest: Authority | Manifest,
    params: unknown,
    template?: Template,
  ) {
    const code = actionSource(manifest, template?.code);
    if (this.usageApiKey) return this.direct(code, params);
    requireThat(
      "owner" in manifest && (params as any)?.document?.kind === "login",
      "Sign in before executing actions",
    );
    const kind = "owner" in manifest ? "authority" : "secret";
    const expected = await cidForCode(code);
    const result = await jsonFetch(
      `${manifest.registry}/api/execute`,
      post({
        kind,
        manifest,
        params,
        ...(template ? { template: template.hash } : {}),
      }),
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
    options: { authorityRelease?: string } = {},
  ) {
    this.authority = authoritySchema.parse(authority);
    this.vaultId = digest(this.authority);
    this.lit = lit;
    this.authorityRelease = options.authorityRelease;
  }
  /**
   * Template hash of the authority release to sign in and approve new objects
   * with. Defaults to the newest release bundled in this client. Recovery tooling
   * and tests can pin an earlier archived release here.
   */
  readonly authorityRelease: string | undefined;
  async api(path: string, init: RequestInit = {}) {
    return jsonFetch(
      this.authority.registry + path,
      { ...init, credentials: "include" },
      this.managementTimeoutMs,
    );
  }
  /**
   * Authority releases are versioned. Secrets pin the release that approved them,
   * so approvals for an existing secret run that exact release; everything else
   * (sign-in, new secrets, credentials) uses the newest release this client knows.
   */
  private async authorityTemplate(authorityCid?: string): Promise<Template> {
    if (authorityCid !== undefined)
      return templateStore.resolve(this.authority, authorityCid);
    return this.authorityRelease === undefined
      ? templateStore.current(this.authority)
      : templateStore.byHash(
          "authority",
          this.authorityRelease,
          this.authority.registry,
        );
  }
  /** CID of the authority release this client signs in and approves new objects with. */
  async currentAuthorityCid() {
    return actionCid(this.authority, (await this.authorityTemplate()).code);
  }
  /** Whether `cid` is a legitimate authority release for this vault. */
  async isAuthorityVersion(cid: string) {
    return templateStore.resolve(this.authority, cid).then(
      () => true,
      () => false,
    );
  }
  async authorize<T extends Document>(
    document: T,
    authorityCid?: string,
  ): Promise<Signed<T>> {
    if (!this.lit.usageApiKey) await this.login();
    const template = await this.authorityTemplate(authorityCid);
    const signed = await this.approve(document, template);
    verifyReceipt(
      signed,
      await this.lit.publicKey(await actionCid(this.authority, template.code)),
      this.vaultId,
    );
    return signed;
  }
  /**
   * Approves several secret objects (manifest, envelope, policy) with a single
   * owner signature and a single authority execution. Each document still comes
   * back with its own exact-object receipt, so storage and verification are
   * unchanged. Authority releases from before batching exist (secrets pinned to
   * them are rotated under that release), so those fall back to one signature
   * per document.
   */
  async authorizeAll<T extends Batch["documents"][number]>(
    documents: T[],
    authorityCid?: string,
  ): Promise<Signed<T>[]> {
    requireThat(documents.length >= 1 && documents.length <= 8);
    if (!this.lit.usageApiKey) await this.login();
    const template = await this.authorityTemplate(authorityCid);
    if (PRE_BATCH_AUTHORITY_HASHES.has(template.hash)) {
      const signed: Signed<T>[] = [];
      for (const document of documents)
        signed.push(await this.authorize(document, authorityCid));
      return signed as any;
    }
    const batch: Batch = { kind: "batch", vaultId: this.vaultId, documents };
    const now = nowSeconds();
    const challenge: Challenge = {
      v: V,
      domain: "lit-keychain/authorize/v2",
      vaultId: this.vaultId,
      objectHash: digest(batch),
      operation: "batch",
      nonce: randomId(),
      issuedAt: now,
      expiresAt: now + 120,
    };
    const proof = await this.signer(challenge);
    const response = await this.lit.execute(
      this.authority,
      { documents, proof },
      template,
    );
    requireThat(
      Array.isArray(response.receipts) &&
        response.receipts.length === documents.length,
      "Authority returned the wrong number of receipts",
    );
    const publicKey = await this.lit.publicKey(
      await actionCid(this.authority, template.code),
    );
    return documents.map((document, i) => {
      const signed = {
        document,
        receipt: receiptSchema.parse(response.receipts[i]),
      };
      verifyReceipt(signed, publicKey, this.vaultId);
      return signed;
    }) as any;
  }
  private async approve<T extends Document>(
    document: T,
    template: Template,
  ): Promise<Signed<T>> {
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
    const response = await this.lit.execute(
      this.authority,
      { document, proof },
      template,
    );
    return { document, receipt: receiptSchema.parse(response.receipt) };
  }
  async login() {
    this.progress?.("Requesting a sign-in challenge…");
    const { authorityCid: recorded, ...document } = await this.api(
      "/auth/challenge",
      post(this.authority),
    );
    requireThat(document.kind === "login" && document.vaultId === this.vaultId);
    this.progress?.("Verifying owner authorization…");
    // Sign in with the newest release, which moves the vault forward. If that
    // release cannot verify this owner (its owner set was approved under the
    // release the vault currently uses), fall back to that release.
    let template = await this.authorityTemplate();
    let authorization: Signed<Document>;
    try {
      authorization = await this.approve(document, template);
    } catch (error) {
      if (
        typeof recorded !== "string" ||
        recorded === (await actionCid(this.authority, template.code))
      )
        throw error;
      template = await templateStore.resolve(this.authority, recorded);
      this.progress?.("Retrying with the vault's current authority release…");
      authorization = await this.approve(document, template);
    }
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
        await this.lit.publicKey(
          await actionCid(this.authority, template.code),
          {
            onWait: () =>
              this.progress?.(
                "Waiting for the Lit network to recognize your new execution key…",
              ),
          },
        ),
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
      authorityCid: await this.currentAuthorityCid(),
      secretId: randomId(),
      release,
    };
    const cid = await actionCid(manifest);
    // Make the derived action executable by this vault's billing key before it
    // is approved, so its encryption key can be fetched and manifest, envelope
    // and policy approved together with one owner signature below.
    this.progress?.("Preparing the secret's action on the Lit network…");
    await this.api("/api/actions/prepare", post({ manifest, actionCid: cid }));
    const manifestDocument = {
      v: V,
      domain: DOMAIN,
      kind: "manifest" as const,
      vaultId: this.vaultId,
      manifest,
      actionCid: cid,
    };
    this.progress?.("Encrypting the secret in your browser…");
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
    const now = nowSeconds();
    const policyDocument: Policy = {
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
    };
    this.progress?.("Approving the secret with one signature…");
    const [signedManifest, signedEnvelope, policy] = (await this.authorizeAll([
      manifestDocument,
      envelope,
      policyDocument,
    ])) as [Signed<typeof manifestDocument>, Signed<Envelope>, Signed<Policy>];
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
      await this.isAuthorityVersion(
        bundle.manifest.document.manifest.authorityCid,
      ),
      "Secret was approved by an authority release this client does not know",
    );
    return bundle;
  }
  /**
   * Longest policy lifetime this secret's pinned release accepts, in days, or
   * null when the owner may choose any lifetime including none.
   */
  async policyLifetimeCapDays(bundle: SecretBundle): Promise<number | null> {
    const template = await this.authorityTemplate(
      bundle.manifest.document.manifest.authorityCid,
    );
    return CAPPED_POLICY_AUTHORITY_HASHES.has(template.hash)
      ? LEGACY_POLICY_CAP_DAYS
      : null;
  }
  /**
   * Re-signs the policy. `days` sets a new expiry that many days from now
   * (default 30); `null` removes the expiry so the policy lasts until the owner
   * revokes or disables it. Secrets pinned to an older release still cap
   * lifetimes at 90 days (see `policyLifetimeCapDays`). `preserveExpiry` keeps
   * the exact existing expiry (including null) and cannot be combined with `days`.
   */
  async setPolicy(
    bundle: SecretBundle,
    changes: {
      grants?: Grant[];
      disabled?: boolean;
      days?: number | null;
      preserveExpiry?: boolean;
    },
  ) {
    const old = bundle.policy.document;
    const now = nowSeconds();
    const days = changes.days === undefined ? 30 : changes.days;
    requireThat(
      days === null ||
        (Number.isInteger(days) &&
          days >= 1 &&
          Number.isSafeInteger(now + days * 86400)),
    );
    requireThat(!(changes.preserveExpiry && changes.days !== undefined));
    const expiresAt = changes.preserveExpiry
      ? old.expiresAt
      : days === null
        ? null
        : now + days * 86400;
    requireThat(expiresAt === null || expiresAt > now);
    const cap = await this.policyLifetimeCapDays(bundle);
    if (cap !== null && (expiresAt === null || expiresAt > now + cap * 86400))
      throw new Error(
        `This secret was created under an earlier Keychain release that limits permissions to ${cap} days. Recreate the secret to choose a longer or unlimited lifetime.`,
      );
    const policy = policySchema.parse({
      ...old,
      epoch: old.epoch + 1,
      previousHash: digest(old),
      notBefore: now,
      expiresAt,
      grants: changes.grants ?? old.grants,
      disabled: changes.disabled ?? old.disabled,
    });
    const signed = await this.authorize(
      policy,
      bundle.manifest.document.manifest.authorityCid,
    );
    await this.api(`/api/secrets/${old.secretId}/policy`, {
      ...post(signed),
      method: "PUT",
    });
    return { ...bundle, policy: signed };
  }
  async delegate(
    bundle: SecretBundle,
    publicKey: string,
    label: string,
    options: { preserveExpiry?: boolean } = {},
  ) {
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
      preserveExpiry: options.preserveExpiry,
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
    const now = nowSeconds();
    const [signedEnvelope, policy] = (await this.authorizeAll(
      [
        envelope,
        {
          ...bundle.policy.document,
          epoch: bundle.policy.document.epoch + 1,
          previousHash: digest(bundle.policy.document),
          notBefore: now,
          // Rotating the value must not shorten (or silently extend) a lifetime
          // the owner already approved, including "never"; an expired policy
          // restarts at the default.
          expiresAt:
            bundle.policy.document.expiresAt === null ||
            bundle.policy.document.expiresAt > now
              ? bundle.policy.document.expiresAt
              : now + 30 * 86400,
          grants: bundle.policy.document.grants.map((g) => ({
            ...g,
            versions: [
              {
                version: envelope.metadata.version,
                envelopeHash: digest(envelope),
              },
            ],
          })),
        },
      ],
      manifest.authorityCid,
    )) as [Signed<Envelope>, Signed<Policy>];
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
    await this.verifyByAnyAuthority(signed);
    return signed;
  }
  /** Verifies a receipt against every authority release this client knows for the vault. */
  private async verifyByAnyAuthority(signed: Signed<unknown>) {
    for (const hash of templateStore.hashes("authority")) {
      try {
        const { code } = await templateStore.byHash(
          "authority",
          hash,
          this.authority.registry,
        );
        verifyReceipt(
          signed,
          await this.lit.publicKey(await actionCid(this.authority, code)),
          this.vaultId,
        );
        return;
      } catch {}
    }
    throw new Error("Receipt was not issued by a known authority release");
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
  /**
   * Deletes a secret. The registry entry every agent request is checked
   * against, all signed policies and every ciphertext version are removed in one
   * transaction, so the next request from any agent is denied and the slot is
   * freed. An encrypted backup taken earlier can still restore it deliberately.
   */
  async deleteSecret(secretId: string) {
    requireThat(/^[0-9a-f]{64}$/.test(secretId), "Invalid secret id");
    await this.api(`/api/secrets/${secretId}`, { method: "DELETE" });
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
    if (lit.usageApiKey) {
      let verified = false;
      for (const hash of templateStore.hashes("authority")) {
        try {
          const { code } = await templateStore.byHash(
            "authority",
            hash,
            authority.registry,
          );
          verifyReceipt(
            credentials,
            await lit.publicKey(await actionCid(authority, code)),
            digest(authority),
          );
          verified = true;
          break;
        } catch {}
      }
      requireThat(verified, "Credentials receipt is from an unknown release");
    }
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
        await this.isAuthorityVersion(
          bundle.manifest.document.manifest.authorityCid,
        ),
        "Backup was approved by an authority release this client does not know",
      );
      await this.api("/api/actions", post(bundle.manifest));
      await this.api("/api/restore", post(bundle));
    }
  }
}
/**
 * Why the signed policy in a bundle would make the action refuse this agent's
 * request, or `undefined` when it permits it. The action never says why it
 * refused (its only failure answer is `access_denied`, so nothing about the
 * policy or the upstream call leaks), so the client explains what it can see
 * before spending an execution.
 */
export function explainDenial(
  policy: Policy,
  agentPublicKey: string,
  operation: string,
  version: number,
  envelopeHash: string,
  now = nowSeconds(),
): string | undefined {
  const when = (seconds: number) => new Date(seconds * 1000).toISOString();
  if (policy.disabled)
    return "the owner disabled this secret. Ask them to enable it in Keychain.";
  if (now < policy.notBefore)
    return `the policy is not valid until ${when(policy.notBefore)}. Check this machine's clock.`;
  if (policy.expiresAt !== null && now >= policy.expiresAt)
    return `the owner's permission expired at ${when(policy.expiresAt)}. Ask them to renew it in Keychain.`;
  const grant = policy.grants.find((g) => g.agentPublicKey === agentPublicKey);
  if (!grant)
    return `agent ${agentPublicKey} is not approved for this secret. Give the owner this public key to approve, or check that the identity file matches the approved agent.`;
  if (!grant.operations.includes(operation))
    return `agent "${grant.label}" is approved for ${grant.operations.join(", ")}, not ${operation}.`;
  if (
    !grant.versions.some(
      (v) => v.version === version && v.envelopeHash === envelopeHash,
    )
  )
    return `agent "${grant.label}" is approved for version ${grant.versions
      .map((v) => v.version)
      .join(
        ", ",
      )} of this secret, not the current version ${version}. Ask the owner to re-approve it after the rotation.`;
  return undefined;
}
/**
 * Message for an `access_denied` answer that arrived even though the signed
 * policy permitted the request. For "use inside Lit" actions that almost always
 * means the upstream call failed, but the enclave does not say.
 */
function refusedByAction(name: string, definition: ActionDefinition) {
  if (definition.kind === "use") {
    const hosts = definition.allowedHosts.join(", ");
    return `Access denied: Lit ran the ${definition.name} action for "${name}" but it did not complete. The owner's policy permits this request, so the call to ${hosts} most likely failed (a rejected or expired credential, or an input the service refused); the enclave reports no detail. Check the credential with its provider or ask the owner to rotate it.`;
  }
  return `Access denied: Lit refused to release "${name}" although the policy this client fetched permits it. The policy may have just changed; retry once, then ask the owner to check this agent's approval in Keychain.`;
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
  const cid = bundle.manifest.document.actionCid;
  // Throws unless some known release of this action binds to exactly this CID.
  await templateStore.resolve(m, cid);
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
  private destroyed = false;
  private assertActive() {
    requireThat(!this.destroyed, "Keychain client has been destroyed");
  }
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
      /**
       * Reuse an attested connection whose usage key already matches this
       * config, instead of attesting a fresh one. Used by the live client.
       */
      lit?: LitConnection;
    } = {},
  ) {
    requireThat(
      !isRecord(privateKey),
      describeCredential(privateKey) === "agent-config"
        ? "The first argument is an agent config (*.keychain.json); pass identity.privateKey first and the config second."
        : "Pass identity.privateKey (the 64-hex string from `keychain init`), not the whole identity object.",
    );
    assertAgentIdentity({ privateKey });
    assertAgentConfig(config);
    if (options.usageApiKey !== undefined)
      assertUsageApiKey(options.usageApiKey);
    this.key = unhex(privateKey);
    this.publicKey = agentPublicKey(this.key);
    const usageApiKey = options.usageApiKey ?? config.usageApiKey;
    if (options.lit) {
      requireThat(
        options.lit.url === origin(config.litApiUrl) &&
          options.lit.usageApiKey === usageApiKey,
        "Shared Lit connection does not match this agent config",
      );
      this.lit = options.lit;
    } else {
      this.lit = new LitConnection(
        config.litApiUrl,
        options.timeoutMs,
        usageApiKey,
        options.attestation,
        { tlsCertificateSha256: options.tlsCertificateSha256 },
      );
    }
  }
  /** Attests the Lit endpoint now instead of lazily on the first read. */
  attest() {
    this.assertActive();
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
    this.destroyed = true;
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
    this.assertActive();
    this.requireSecret(name);
    const locator = this.config.secrets[name];
    const definition = actionDefinition(locator.manifest.release);
    requireThat(
      definition.kind === "use",
      `Secret "${name}" is a stored secret; call get("${name}") or \`keychain run\` instead of use(). Only connected services have an action to run.`,
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
  /** Names the config's secrets in the error so a typo is obvious. */
  private requireSecret(name: string) {
    const names = Object.keys(this.config.secrets);
    requireThat(
      Object.hasOwn(this.config.secrets, name),
      `Unknown secret "${name}". This agent config contains: ${names.join(", ") || "no secrets"}`,
    );
  }
  private async read(
    name: string,
    operation: string,
    input?: Record<string, unknown>,
  ): Promise<string> {
    this.assertActive();
    this.requireSecret(name);
    const locator = this.config.secrets[name];
    const manifest = manifestSchema.parse(locator.manifest);
    // The exact release this secret was created under; fetched by hash if older
    // than this client's bundled template.
    const template = await templateStore.resolve(manifest, locator.actionCid);
    const definition = actionDefinition(manifest.release);
    requireThat(
      definition.operation === operation,
      definition.kind === "use"
        ? `Secret "${name}" was created with the ${definition.name} action (${manifest.release}); call use("${name}") instead of get(). Its value never leaves the enclave.`
        : `Secret "${name}" is a stored secret; call get("${name}") instead of use().`,
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
    // The action answers every failure with a bare access_denied. Explain what
    // the signed policy already shows before paying for an execution.
    const denial = explainDenial(
      bundle.policy.document,
      this.publicKey,
      operation,
      bundle.envelope.document.metadata.version,
      digest(bundle.envelope.document),
      now,
    );
    requireThat(denial === undefined, `Access denied: ${denial}`);
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
      let result;
      try {
        result = await this.lit.execute(
          manifest,
          {
            operation,
            signedRequest: { request, signature: signAgent(request, this.key) },
            envelope: bundle.envelope,
          },
          template,
        );
      } catch (error) {
        if (error instanceof Error && error.message === "Access denied")
          throw new Error(refusedByAction(name, definition));
        throw error;
      }
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
