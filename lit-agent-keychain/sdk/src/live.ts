import {
  Keychain,
  LitConnection,
  DEFAULT_LIT_API_URL,
  DEFAULT_KEYCHAIN_SERVICE_URL,
  assertAgentIdentity,
  assertUsageApiKey,
  verifyBundle,
  explainDenial,
  actionDefinition,
  type AttestationOption,
  type SecretBundle,
  type Manifest,
  type Shape,
} from "./index.ts";
import {
  agentPublicKey,
  unhex,
  hex,
  signAgent,
  nowSeconds,
  requireThat,
  digest,
} from "../../protocol/crypto.ts";
import { manifestSchema } from "../../protocol/schema.ts";
import { jsonFetch } from "../../protocol/client-http.ts";

export type LiveKeychainOptions = {
  serviceUrl?: string;
  /** A local trust decision, NEVER supplied by discovery. */
  litApiUrl?: string;
  timeoutMs?: number;
  attestation?: AttestationOption;
  tlsCertificateSha256?: string;
};
type Locator = {
  name: string;
  manifest: Manifest;
  actionCid: string;
  usageApiKey: string;
};
export type LiveSecretInfo = {
  id: string;
  name: string;
  vaultId: string;
  secretId: string;
  release: string;
  operation: string;
  input?: Shape | null;
};
const post = (body: unknown): RequestInit => ({
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(body),
  cache: "no-store",
  credentials: "omit",
});
function serviceOrigin(url: string) {
  const u = new URL(url);
  requireThat(
    u.origin === url &&
      (u.protocol === "https:" ||
        (u.protocol === "http:" &&
          ["localhost", "127.0.0.1", "[::1]"].includes(u.hostname))),
    "Invalid service origin",
  );
  return url;
}

/** Live agent client: no per-agent config, inventory cache or mutable Lit trust. */
export class LiveKeychain {
  private readonly key: Uint8Array<ArrayBuffer>;
  private destroyed = false;
  private readonly connections = new Map<string, LitConnection>();
  readonly publicKey: string;
  readonly serviceUrl: string;
  readonly lit: LitConnection;
  constructor(
    privateKey: string,
    private readonly options: LiveKeychainOptions = {},
  ) {
    assertAgentIdentity({ privateKey });
    this.key = unhex(privateKey);
    this.publicKey = agentPublicKey(this.key);
    this.serviceUrl = serviceOrigin(
      options.serviceUrl ?? DEFAULT_KEYCHAIN_SERVICE_URL,
    );
    this.lit = new LitConnection(
      options.litApiUrl ?? DEFAULT_LIT_API_URL,
      options.timeoutMs,
      undefined,
      options.attestation,
      { tlsCertificateSha256: options.tlsCertificateSha256 },
    );
  }
  private active() {
    requireThat(!this.destroyed, "Keychain client has been destroyed");
  }
  attest() {
    this.active();
    return this.lit.attest();
  }
  destroy() {
    this.destroyed = true;
    this.connections.clear();
    this.key.fill(0);
  }
  private async discover(): Promise<Locator[]> {
    this.active();
    const challenge = await jsonFetch(
      `${this.serviceUrl}/api/agents/challenge`,
      post({ agentPublicKey: this.publicKey }),
      this.lit.timeoutMs,
    );
    const now = nowSeconds();
    requireThat(
      challenge &&
        Object.keys(challenge).sort().join(",") ===
          "agentPublicKey,audience,domain,expiresAt,issuedAt,nonce,v" &&
        challenge.v === 2 &&
        challenge.domain === "lit-keychain/discovery/v2" &&
        challenge.audience === this.serviceUrl &&
        challenge.agentPublicKey === this.publicKey &&
        /^[0-9a-f]{64}$/.test(challenge.nonce) &&
        Number.isSafeInteger(challenge.issuedAt) &&
        challenge.issuedAt >= 0 &&
        challenge.issuedAt <= now + 30 &&
        Number.isSafeInteger(challenge.expiresAt) &&
        challenge.expiresAt > now &&
        challenge.expiresAt > challenge.issuedAt &&
        challenge.expiresAt - challenge.issuedAt <= 60,
      "Invalid discovery challenge",
    );
    this.active();
    const result = await jsonFetch(
      `${this.serviceUrl}/api/agents/discover`,
      post({ challenge, signature: signAgent(challenge, this.key) }),
      this.lit.timeoutMs,
    );
    requireThat(
      result?.v === 2 &&
        Array.isArray(result.secrets) &&
        result.secrets.length <= 1000,
      "Invalid discovery response",
    );
    const seen = new Set<string>();
    const locators: Locator[] = [];
    for (const row of result.secrets) {
      const manifest = manifestSchema.parse(row.manifest);
      requireThat(
        manifest.registry === this.serviceUrl &&
          typeof row.actionCid === "string" &&
          /^[A-Z][A-Z0-9_]{0,63}$/.test(row.name),
        "Invalid discovery locator",
      );
      assertUsageApiKey(row.usageApiKey);
      const id = `${manifest.vaultId}/${manifest.secretId}`;
      requireThat(!seen.has(id), "Duplicate discovery locator");
      seen.add(id);
      locators.push({
        name: row.name,
        manifest,
        actionCid: row.actionCid,
        usageApiKey: row.usageApiKey,
      });
    }
    this.active();
    return locators;
  }
  /**
   * One connection per execution key, kept for the client's lifetime so the
   * attestation and public-key lookups are done once per vault rather than
   * once per secret per call. A vault whose key changes simply gets a new one.
   */
  private connection(locator: Locator) {
    const cached = this.connections.get(locator.usageApiKey);
    if (cached) return cached;
    const connection = new LitConnection(
      this.lit.url,
      this.lit.timeoutMs,
      locator.usageApiKey,
      this.options.attestation,
      { tlsCertificateSha256: this.options.tlsCertificateSha256 },
    );
    this.connections.set(locator.usageApiKey, connection);
    return connection;
  }
  private async verified(locator: Locator) {
    const bundle: SecretBundle = await jsonFetch(
      `${this.serviceUrl}/api/secrets/${locator.manifest.secretId}/bundle`,
      { cache: "no-store", credentials: "omit" },
      this.lit.timeoutMs,
    );
    await verifyBundle(
      bundle,
      this.connection(locator),
      locator.manifest.vaultId,
    );
    requireThat(
      digest(bundle.manifest.document.manifest) === digest(locator.manifest) &&
        bundle.manifest.document.actionCid === locator.actionCid &&
        bundle.envelope.document.metadata.name === locator.name,
      "Discovery substitution",
    );
    return explainDenial(
      bundle.policy.document,
      this.publicKey,
      actionDefinition(locator.manifest.release).operation,
      bundle.envelope.document.metadata.version,
      digest(bundle.envelope.document),
    );
  }
  /** Always fresh. Use id for unambiguous access; bare names work only if unique. */
  async list() {
    const rows = await this.discover();
    const result: LiveSecretInfo[] = [];
    for (const row of rows) {
      // A revocation racing discovery simply removes the entry; invalid receipts
      // fail the whole request instead of silently trusting registry metadata.
      if (await this.verified(row)) continue;
      const definition = actionDefinition(row.manifest.release);
      result.push({
        id: `${row.manifest.vaultId}/${row.manifest.secretId}`,
        name: row.name,
        vaultId: row.manifest.vaultId,
        secretId: row.manifest.secretId,
        release: row.manifest.release,
        operation: definition.operation,
        ...(definition.kind === "use" ? { input: definition.input } : {}),
      });
    }
    this.active();
    return result;
  }
  private async run(
    name: string,
    operation: "get" | "use",
    input?: Record<string, unknown>,
  ) {
    const matches = (await this.discover()).filter(
      (row) =>
        row.name === name ||
        `${row.manifest.vaultId}/${row.manifest.secretId}` === name,
    );
    requireThat(
      matches.length !== 0,
      `Unknown secret "${name}". No current approval; call list() and ask the owner to approve this public key.`,
    );
    requireThat(
      matches.length === 1,
      `Secret name "${name}" is ambiguous across vaults; use its vaultId/secretId from list().`,
    );
    const locator = matches[0];
    const denial = await this.verified(locator);
    requireThat(denial === undefined, `Access denied: ${denial}`);
    this.active();
    const client = new Keychain(
      hex(this.key),
      {
        v: 2,
        litApiUrl: this.lit.url,
        usageApiKey: locator.usageApiKey,
        secrets: {
          [name]: { manifest: locator.manifest, actionCid: locator.actionCid },
        },
      },
      { ...this.options, lit: this.connection(locator) },
    );
    try {
      return operation === "get"
        ? await client.get(name)
        : await client.use(name, input);
    } finally {
      client.destroy();
    }
  }
  get(name: string): Promise<string> {
    return this.run(name, "get");
  }
  use(name: string, input?: Record<string, unknown>): Promise<any> {
    return this.run(name, "use", input);
  }
  stripeBalance(name: string) {
    return this.use(name);
  }
}
