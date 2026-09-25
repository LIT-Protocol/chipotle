// Test-only identity/transport boundary. Never included by the production Vite config.
import type { SecretBundle } from "../../sdk/src/index.ts";
export const LIT_URL = "https://lit.invalid";
const owner = { kind: "wallet", address: "0x" + "11".repeat(20) };
export const createPasskey = async () => ({ owner });
export const discoverPasskey = async () => ({ identity: { owner } });
export const walletIdentity = () => ({ owner });
export const googleSession = () => {
  throw new Error("Google is not used by this fixture");
};
export const passkeyIdentity = () => ({ owner, signer: async () => ({}) });
export class GoogleSessionExpired extends Error {}
export const saveSession = () => {};
export const loadSession = () => null;
export const clearSession = () => {};
export const sessionAlive = async () => false;
const bundles: Record<string, SecretBundle> = Object.fromEntries(
  ["ONE", "TWO"].map((name) => [
    name,
    {
      manifest: {
        document: {
          manifest: { secretId: name, release: "export" },
          actionCid: "fixture",
        },
      },
      envelope: { document: { metadata: { name, version: 1 } } },
      policy: { document: { grants: [], disabled: false, expiresAt: null } },
    } as unknown as SecretBundle,
  ]),
);
(window as any).approvals = [];
export const ownerClient = () => ({
  vaultId: "11".repeat(32),
  authority: { owner },
  lit: { usageApiKey: "fixture-execution-key" },
  login: async () => {},
  getCredentials: async () => null,
  api: async () => ({
    subscription: { active: false, secretLimit: 5 },
    canManageBilling: false,
  }),
  listSecrets: async () =>
    Object.entries(bundles).map(([id, b]) => ({
      secretId: id,
      name: id,
      release: "export",
      version: 1,
      disabled: false,
      expiresAt: null,
      agentCount: b.policy.document.grants.length,
      agents: b.policy.document.grants,
    })),
  bundle: async (id: string) => bundles[id],
  policyLifetimeCapDays: async () => null,
  delegate: async (b: SecretBundle, key: string, label: string) => {
    const id = b.manifest.document.manifest.secretId;
    (window as any).approvals.push(id);
    bundles[id] = {
      ...b,
      policy: {
        ...b.policy,
        document: {
          ...b.policy.document,
          grants: [{ agentPublicKey: key, label }],
        },
      },
    } as SecretBundle;
    return bundles[id];
  },
});
