import React, {
  useState,
  useEffect,
  useRef,
  useCallback,
  useMemo,
} from "react";
import { createRoot } from "react-dom/client";
import {
  WagmiProvider,
  useAccount,
  useSignTypedData,
  useDisconnect,
  http,
  createConfig,
} from "wagmi";
import { mainnet, base } from "wagmi/chains";
import { injectedWallet } from "@rainbow-me/rainbowkit/wallets";
import {
  RainbowKitProvider,
  ConnectButton,
  getDefaultConfig,
  lightTheme,
  connectorsForWallets,
} from "@rainbow-me/rainbowkit";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  OwnerClient,
  LitConnection,
  Keychain,
  digest,
  ACTIONS,
  type Authority,
  type Grant,
  type SecretBundle,
  type AgentConfig,
} from "../../sdk/src/index.ts";
import { jsonFetch } from "../../protocol/client-http.ts";
import { ownerSchema, type Owner } from "../../protocol/schema.ts";
import {
  ownerClient,
  walletIdentity,
  createPasskey,
  discoverPasskey,
  googleSession,
  LIT_URL,
  type Identity,
} from "./identities.ts";
import {
  Brand,
  Intro,
  LandingFooter,
  LitMark,
  KEYCHAIN_DOCS_URL,
} from "./Landing.tsx";
import { NPX_KEYCHAIN } from "./version.ts";
import { AddSecret, ActionDocs } from "./AddSecret.tsx";
import { AgentOnboarding } from "./AgentOnboarding.tsx";
import { downloadAgentConfig } from "./agent-config.ts";
import "@rainbow-me/rainbowkit/styles.css";
import "./style.css";

const projectId = import.meta.env.VITE_WALLETCONNECT_PROJECT_ID;
const config = projectId
  ? getDefaultConfig({
      appName: "Lit Agent Keychain",
      projectId,
      chains: [mainnet, base],
      ssr: false,
    })
  : createConfig({
      chains: [mainnet, base],
      connectors: connectorsForWallets(
        [{ groupName: "Browser wallet", wallets: [injectedWallet] }],
        { appName: "Lit Agent Keychain", projectId: "unused-injected-only" },
      ),
      transports: { [mainnet.id]: http(), [base.id]: http() },
    });
const queryClient = new QueryClient();
function download(name: string, value: unknown) {
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(value, null, 2)], { type: "application/json" }),
  );
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  URL.revokeObjectURL(url);
}
async function readFile(file: File) {
  if (file.size > 192 * 1024 * 1024) throw new Error("Backup is too large");
  return JSON.parse(await file.text());
}
const brief = (s: string) => s.slice(0, 8) + "…" + s.slice(-6);
type AgentSummary = {
  key: string;
  label: string;
  labels: string[];
  secrets: any[];
};
/** Short label for a secret's action: how agents may use it. */
const actionLabel = (release: string) =>
  release === "export"
    ? "Stored secret"
    : (ACTIONS[release]?.name ?? "Connected service");
const isStored = (release: string) => release === "export";
function GoogleButton({
  clientId,
  network,
  onIdentity,
  onError,
}: {
  clientId: string;
  network: string;
  onIdentity: (identity: Identity) => void;
  onError: (error: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const callbacks = useRef({ onIdentity, onError });
  callbacks.current = { onIdentity, onError };
  useEffect(() => {
    const session = googleSession(network);
    let active = true;
    const init = () => {
      const google = (window as any).google;
      if (!active || !google || !ref.current) return;
      google.accounts.id.initialize({
        client_id: clientId,
        nonce: session.nonce,
        auto_select: false,
        callback: (response: { credential: string }) => {
          try {
            if (active)
              callbacks.current.onIdentity(
                session.identity(response.credential, clientId),
              );
          } catch {
            callbacks.current.onError(
              "Google sign-in failed. Please try again.",
            );
          }
        },
      });
      google.accounts.id.renderButton(ref.current, {
        theme: "outline",
        size: "large",
        text: "continue_with",
        width: Math.max(200, Math.min(400, ref.current.clientWidth || 400)),
      });
    };
    if ((window as any).google) init();
    else {
      const script = document.createElement("script");
      script.src = "https://accounts.google.com/gsi/client";
      script.async = true;
      script.onload = init;
      script.onerror = () =>
        callbacks.current.onError("Unable to load Google sign-in");
      document.head.appendChild(script);
    }
    // The selected identity owns its in-memory session until sign-out. Never persist the token/key.
    return () => {
      active = false;
    };
  }, [clientId, network]);
  return <div ref={ref} className="google-button" />;
}
function App() {
  const [settings, setSettings] = useState<any>();
  const [client, setClient] = useState<OwnerClient>();
  const [recovery, setRecovery] = useState<Authority>();
  const [secrets, setSecrets] = useState<any[]>([]);
  const [billing, setBilling] = useState<any>();
  const [selected, setSelected] = useState<SecretBundle>();
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [tab, setTab] = useState<
    "secrets" | "agents" | "recovery" | "activity"
  >("secrets");
  const [events, setEvents] = useState<any[]>([]);
  const [creating, setCreating] = useState(false);
  const [addingAgent, setAddingAgent] = useState<
    false | { name?: string; key?: string }
  >(false);
  const [selectedAgent, setSelectedAgent] = useState("");
  const addAgentButton = useRef<HTMLButtonElement>(null);
  const [agentKey, setAgentKey] = useState("");
  const [agentName, setAgentName] = useState("");
  const [rotation, setRotation] = useState("");
  const [days, setDays] = useState(30);
  // 90 for secrets pinned to an older release that still caps lifetimes; null = owner's choice.
  const [lifetimeCap, setLifetimeCap] = useState<number | null>(null);
  const [recoveryOwners, setRecoveryOwners] = useState<Owner[]>([]);
  const [newWallet, setNewWallet] = useState("");
  const { address } = useAccount();
  const { signTypedDataAsync } = useSignTypedData();
  const { disconnect } = useDisconnect();
  useEffect(() => {
    jsonFetch("/api/config")
      .then(setSettings)
      .catch(() => setError("Unable to connect to Keychain."));
  }, []);
  const work = async (label: string, operation: () => Promise<void>) => {
    if (busy) return;
    setBusy(label);
    setError("");
    setNotice("");
    try {
      await operation();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Operation failed");
    } finally {
      setBusy("");
    }
  };
  const refresh = async (c = client) => {
    if (c) {
      const [stored, billing] = await Promise.all([
        c.listSecrets(),
        c.api("/api/billing"),
      ]);
      setSecrets(stored);
      setBilling(billing);
    }
  };
  const signIn = async (identity: Identity, authority?: Authority) =>
    work("Verifying owner authorization…", async () => {
      const c = ownerClient(identity, settings.network, authority || recovery);
      c.progress = setBusy;
      await c.login();
      setBusy("Loading your vault…");
      setClient(c);
      await c.api("/api/billing/refresh", { method: "POST" });
      await refresh(c);
      const policy = await c.getCredentials();
      setRecoveryOwners(policy?.document.owners || [c.authority.owner]);
      setNotice("Signed in. Owner approvals stay on this device.");
    });
  const pick = (id: string) =>
    work("Checking secret…", async () => {
      const bundle = await client!.bundle(id);
      setLifetimeCap(await client!.policyLifetimeCapDays(bundle));
      setSelected(bundle);
      setCreating(false);
      setRotation("");
    });
  const savePolicy = async (changes: {
    grants?: Grant[];
    disabled?: boolean;
    days?: number | null;
  }) => {
    const updated = await client!.setPolicy(selected!, changes);
    setSelected(updated);
    await refresh();
  };
  const atLimit =
    !!billing && secrets.length >= billing.subscription.secretLimit;
  /** The Secrets listing inverted: every approved public key with the secrets it may use.
   *  Labels are per secret, so one key may carry several; the first is the display name. */
  const agents = useMemo(() => {
    const byKey = new Map<string, AgentSummary>();
    for (const s of secrets)
      for (const g of (s.agents ?? []) as Grant[]) {
        let a = byKey.get(g.agentPublicKey);
        if (!a) {
          a = {
            key: g.agentPublicKey,
            label: g.label,
            labels: [],
            secrets: [],
          };
          byKey.set(g.agentPublicKey, a);
        }
        if (!a.labels.includes(g.label)) a.labels.push(g.label);
        a.secrets.push(s);
      }
    return [...byKey.values()].sort((a, b) => a.label.localeCompare(b.label));
  }, [secrets]);
  const agent = agents.find((a) => a.key === selectedAgent);
  const revokeFromSecret = (agentPublicKey: string, secretId: string) =>
    work("Revoking agent…", async () => {
      const bundle = await client!.bundle(secretId);
      const updated = await client!.setPolicy(bundle, {
        grants: bundle.policy.document.grants.filter(
          (g) => g.agentPublicKey !== agentPublicKey,
        ),
      });
      if (selected?.manifest.document.manifest.secretId === secretId)
        setSelected(updated);
      await refresh();
    });
  const locatorOf = (bundle: SecretBundle) => ({
    manifest: bundle.manifest.document.manifest,
    actionCid: bundle.manifest.document.actionCid,
  });
  const agentConfig = (secrets: AgentConfig["secrets"]): AgentConfig => ({
    v: 2,
    litApiUrl: LIT_URL,
    usageApiKey: client!.lit.usageApiKey,
    secrets,
  });
  const exportConfig = (bundle: SecretBundle) => {
    const name = bundle.envelope.document.metadata.name;
    const file = `${name}.keychain.json`;
    download(file, agentConfig({ [name]: locatorOf(bundle) }));
    setError("");
    setNotice(
      `Downloaded ${file}. It names this secret and carries the execution key, no secret value; give it to the agent next to its identity file.`,
    );
  };
  /** One config listing every secret this agent is approved for, so `keychain run`
   *  and `get`/`use` need a single file (the MCP server can also merge several). */
  const exportAgentConfig = (agent: Pick<Grant, "agentPublicKey" | "label">) =>
    work("Collecting this agent's secrets…", async () => {
      const approved: AgentConfig["secrets"] = {};
      for (const s of secrets) {
        const bundle: SecretBundle =
          selected &&
          selected.manifest.document.manifest.secretId === s.secretId
            ? selected
            : await client!.bundle(s.secretId);
        if (
          !bundle.policy.document.grants.some(
            (g) => g.agentPublicKey === agent.agentPublicKey,
          )
        )
          continue;
        approved[bundle.envelope.document.metadata.name] = locatorOf(bundle);
      }
      const names = Object.keys(approved);
      const file = `${
        agent.label.replace(/[^A-Za-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "") ||
        "agent"
      }.keychain.json`;
      download(file, agentConfig(approved));
      setNotice(
        `Downloaded ${file} for ${agent.label}: ${names.length} secret${
          names.length === 1 ? "" : "s"
        } (${names.join(", ")}). It carries no secret values.`,
      );
    });
  const reveal = () => {
    if (!selected) return;
    const secretName = selected.envelope.document.metadata.name;
    if (
      !window.confirm(
        `Export "${secretName}" in plaintext?\n\nThis decrypts the current value and saves it unencrypted as ${secretName}.json in your downloads folder. Delete that file once you have used it. For agents, approve their public key instead; live clients need no config download.`,
      )
    )
      return;
    void work("Authorizing a temporary reader…", async () => {
      const key = Keychain.generateKey();
      const original = selected.policy.document.grants;
      let delegated: SecretBundle | undefined;
      try {
        delegated = await client!.delegate(
          selected,
          key.publicKey,
          "Owner recovery",
        );
        setSelected(delegated);
        const m = delegated.manifest.document.manifest;
        const secretName = delegated.envelope.document.metadata.name;
        const agent = new Keychain(key.privateKey, {
          v: 2,
          litApiUrl: LIT_URL,
          usageApiKey: client!.lit.usageApiKey,
          secrets: {
            [secretName]: {
              manifest: m,
              actionCid: delegated.manifest.document.actionCid,
            },
          },
        });
        try {
          const plaintext = await agent.get(secretName);
          download(`${secretName}.json`, {
            name: secretName,
            value: plaintext,
          });
          setNotice(
            `Saved the plaintext of ${secretName} as ${secretName}.json in your downloads folder. Delete it when you are done.`,
          );
        } finally {
          agent.destroy();
        }
      } finally {
        if (delegated) {
          const restored = await client!.setPolicy(delegated, {
            grants: original,
          });
          setSelected(restored);
          await refresh();
        }
      }
    });
  };
  return (
    <>
      <header className="topbar">
        <Brand />
        <div className="header-right">
          <a href={KEYCHAIN_DOCS_URL}>
            Developer docs <span aria-hidden="true">↗</span>
          </a>
          {client && (
            <button
              className="ghost"
              disabled={!!busy}
              onClick={() =>
                work("Signing out…", async () => {
                  await fetch("/auth/logout", { method: "POST" });
                  client.lit.usageApiKey = undefined;
                  setClient(undefined);
                  setTab("secrets");
                  setAddingAgent(false);
                  setSelectedAgent("");
                  setSelected(undefined);
                  setSecrets([]);
                  setBilling(undefined);
                  disconnect();
                })
              }
            >
              Sign out
            </button>
          )}
        </div>
      </header>
      {busy && (
        <div className="progress" role="status" aria-live="polite">
          <span className="spinner" aria-hidden="true" />
          {busy}
        </div>
      )}
      {error && (
        <div className="alert error" role="alert">
          {error}
          <button onClick={() => setError("")} aria-label="Dismiss error">
            ×
          </button>
        </div>
      )}
      {notice && (
        <div className="alert success" role="status">
          {notice}
        </div>
      )}
      {!client ? (
        <>
          <main className="login-layout">
            <Intro />
            <section className="signin" aria-labelledby="signin-title">
              <p className="eyebrow">Your vault</p>
              <h2 id="signin-title">Sign in to Keychain.</h2>
              <p className="muted">
                Google, a wallet or a passkey. Each can own a vault; Google
                needs neither a wallet nor a passkey.
              </p>
              <div className="signin-methods">
                {settings?.googleClientId && (
                  <GoogleButton
                    clientId={settings.googleClientId}
                    network={settings.network}
                    onIdentity={(identity) => void signIn(identity)}
                    onError={setError}
                  />
                )}
                <div className="wallet-row">
                  <ConnectButton chainStatus="none" showBalance={false} />
                  {address && (
                    <button
                      disabled={!!busy || !settings}
                      onClick={() =>
                        void signIn(
                          walletIdentity(address, signTypedDataAsync as any),
                        )
                      }
                    >
                      Sign in with wallet
                    </button>
                  )}
                </div>
                <p className="divider-label">or use a passkey</p>
                <button
                  className="secondary"
                  disabled={!!busy || !settings}
                  onClick={() =>
                    work("Creating a passkey…", async () => {
                      const identity = await createPasskey("My Keychain");
                      const c = ownerClient(
                        identity,
                        settings.network,
                        recovery,
                      );
                      c.progress = setBusy;
                      await c.login();
                      setBusy("Loading your vault…");
                      setClient(c);
                      await c.api("/api/billing/refresh", { method: "POST" });
                      setRecoveryOwners([identity.owner]);
                      await refresh(c);
                    })
                  }
                >
                  Create a passkey
                </button>
                <button
                  className="ghost"
                  disabled={!!busy || !settings}
                  onClick={() =>
                    work("Finding your passkey…", async () => {
                      const found = await discoverPasskey(recovery);
                      const c = ownerClient(
                        found.identity,
                        settings.network,
                        // An explicitly loaded backup selects the vault, even if lookup
                        // finds an empty duplicate rooted at the recovery credential.
                        recovery || found.authority,
                      );
                      c.progress = setBusy;
                      await c.login();
                      setBusy("Loading your vault…");
                      setClient(c);
                      await c.api("/api/billing/refresh", { method: "POST" });
                      await refresh(c);
                      const policy = await c.getCredentials();
                      setRecoveryOwners(
                        policy?.document.owners || [c.authority.owner],
                      );
                    })
                  }
                >
                  Use an existing passkey
                </button>
              </div>
              <details>
                <summary>Recover an existing vault</summary>
                <p>
                  Choose the backup file you downloaded from this vault, then
                  sign in with an approved credential.
                </p>
                <input
                  aria-label="Backup file"
                  type="file"
                  accept="application/json"
                  onChange={(e) => {
                    const f = e.target.files?.[0];
                    if (f)
                      void work("Reading backup file…", async () => {
                        const data = await readFile(f);
                        await OwnerClient.restoreCredentials(
                          data,
                          new LitConnection(LIT_URL),
                        );
                        setRecovery(data.authority);
                        setNotice(
                          "Backup loaded. Sign in with one of this vault's approved credentials.",
                        );
                      });
                  }}
                />
                {recovery && <p>Vault {brief(digest(recovery))} selected.</p>}
              </details>
            </section>
          </main>
        </>
      ) : (
        <div className="workspace">
          <aside>
            <div className="account">
              <span className="account-mark" aria-hidden="true">
                <LitMark />
              </span>
              <div>
                <strong>Your vault</strong>
                <code>{brief(client.vaultId)}</code>
              </div>
            </div>
            <nav aria-label="Vault">
              {(["secrets", "agents", "recovery", "activity"] as const).map(
                (t) => (
                  <button
                    key={t}
                    disabled={!!busy}
                    className={tab === t ? "active" : ""}
                    onClick={() => {
                      setTab(t);
                      if (t === "activity")
                        void work("Loading activity…", async () =>
                          setEvents((await client.api("/api/audit")).events),
                        );
                    }}
                  >
                    {t === "secrets"
                      ? "Secrets"
                      : t === "agents"
                        ? "Agents"
                        : t === "recovery"
                          ? "Recovery & backups"
                          : "Activity"}
                  </button>
                ),
              )}
            </nav>
            <p className="aside-note">
              Permissions are verified in Lit Actions. Revocation freshness is
              trusted to Keychain.
            </p>
          </aside>
          <main className="dashboard">
            <section className="billing-panel" aria-label="Subscription">
              <div>
                <strong>
                  {billing?.subscription?.plan === "custom"
                    ? "Custom plan"
                    : billing?.subscription?.active
                      ? "$10/month · Standard"
                      : "Free"}
                </strong>
                <p>
                  {secrets.length.toLocaleString()} /{" "}
                  {(billing?.subscription?.secretLimit ?? 5).toLocaleString()}{" "}
                  secrets
                </p>
                <small>
                  Execution included under fair use. No automatic overage
                  charges.
                </small>
                {billing?.subscription?.active ? (
                  <small>
                    {billing.subscription.cancelAtPeriodEnd
                      ? "Cancels"
                      : "Access paid through"}{" "}
                    {new Date(
                      billing.subscription.paidUntil * 1000,
                    ).toLocaleDateString()}
                  </small>
                ) : (
                  <small>
                    Free includes 5 secrets. Subscribe for up to 1,000. Your
                    secrets and encrypted backups are never deleted.
                  </small>
                )}
              </div>
              <div className="billing-actions">
                {!billing?.subscription?.active && (
                  <button
                    disabled={!!busy || !billing}
                    onClick={() =>
                      work("Opening secure checkout…", async () => {
                        const { url } = await client.api(
                          "/api/billing/checkout",
                          { method: "POST" },
                        );
                        window.location.assign(url);
                      })
                    }
                  >
                    Subscribe for $10/month
                  </button>
                )}
                {billing?.canManageBilling && (
                  <button
                    className="secondary"
                    disabled={!!busy}
                    onClick={() =>
                      work("Opening billing management…", async () => {
                        const { url } = await client.api(
                          "/api/billing/portal",
                          { method: "POST" },
                        );
                        window.location.assign(url);
                      })
                    }
                  >
                    Manage billing
                  </button>
                )}
                <button
                  className="ghost"
                  disabled={!!busy}
                  onClick={() =>
                    work("Refreshing subscription…", async () => {
                      await client.api("/api/billing/refresh", {
                        method: "POST",
                      });
                      await refresh();
                    })
                  }
                >
                  Refresh billing
                </button>
                <a
                  href={`mailto:${encodeURIComponent(billing?.contactEmail || "support@litprotocol.com")}?subject=Keychain%20custom%20plan`}
                >
                  Contact us for more
                </a>
              </div>
            </section>
            {tab === "secrets" && (
              <>
                <div className="page-heading">
                  <div>
                    <p className="eyebrow">AGENT ACCESS</p>
                    <h1>Secrets</h1>
                    <p>
                      {atLimit
                        ? `This plan holds ${billing.subscription.secretLimit} secrets and all are in use. Subscribe above to add more, or rotate an existing secret instead of adding one.`
                        : "Start with no agent access. Grant only what each agent needs."}
                    </p>
                  </div>
                  <div className="button-row">
                    <button
                      disabled={!!busy || !billing || atLimit}
                      title={
                        atLimit
                          ? `Plan full: ${secrets.length} of ${billing.subscription.secretLimit} secrets used`
                          : undefined
                      }
                      onClick={() => {
                        setCreating(true);
                        setSelected(undefined);
                      }}
                    >
                      + Add secret
                    </button>
                  </div>
                </div>
                <div className="secret-layout">
                  <section className="secret-list">
                    {secrets.length === 0 && (
                      <div className="empty">
                        <h3>Your vault is ready</h3>
                        <p>
                          Add your first secret, then authorize an agent’s
                          public key.
                        </p>
                      </div>
                    )}
                    {[
                      {
                        title: "Stored secrets",
                        note: "Agents receive an encrypted copy.",
                        items: secrets.filter((s) => isStored(s.release)),
                      },
                      {
                        title: "Connected services",
                        note: "Agents run one reviewed action; they never see the key.",
                        items: secrets.filter((s) => !isStored(s.release)),
                      },
                    ]
                      .filter((group) => group.items.length > 0)
                      .map((group) => (
                        <div className="secret-group" key={group.title}>
                          {secrets.some((s) => isStored(s.release)) &&
                            secrets.some((s) => !isStored(s.release)) && (
                              <p className="eyebrow">
                                {group.title.toUpperCase()}
                                <span className="muted"> · {group.note}</span>
                              </p>
                            )}
                          {group.items.map((s) => (
                            <button
                              className={
                                "secret-row " +
                                (selected?.manifest.document.manifest
                                  .secretId === s.secretId
                                  ? "selected"
                                  : "")
                              }
                              key={s.secretId}
                              onClick={() => pick(s.secretId)}
                            >
                              <span
                                className={
                                  "secret-icon" +
                                  (isStored(s.release) ? "" : " service")
                                }
                              >
                                {isStored(s.release) ? "⌘" : "⚡"}
                              </span>
                              <span>
                                <strong>{s.name}</strong>
                                <small>
                                  {actionLabel(s.release)} · v{s.version}
                                </small>
                              </span>
                              <span
                                className={
                                  "status " + (s.disabled ? "off" : "")
                                }
                              >
                                {s.disabled
                                  ? "Disabled"
                                  : s.expiresAt !== null &&
                                      s.expiresAt * 1000 <= Date.now()
                                    ? "Expired"
                                    : s.agentCount === 1
                                      ? "1 agent"
                                      : s.agentCount + " agents"}
                              </span>
                            </button>
                          ))}
                        </div>
                      ))}
                  </section>
                  <section className="detail-card">
                    {creating ? (
                      <AddSecret
                        busy={!!busy}
                        onCancel={() => setCreating(false)}
                        onCreate={(name, value, release) =>
                          work("Encrypting and approving secret…", async () => {
                            const bundle = await client.create(
                              name,
                              value,
                              release,
                            );
                            setCreating(false);
                            setLifetimeCap(null);
                            setSelected(bundle);
                            await refresh();
                            setNotice(
                              isStored(release)
                                ? "Secret saved. No agents have access yet."
                                : "Service connected. No agents have access yet.",
                            );
                          })
                        }
                      />
                    ) : selected ? (
                      <>
                        <p className="eyebrow">
                          {isStored(selected.manifest.document.manifest.release)
                            ? "STORED SECRET · AGENTS RECEIVE AN ENCRYPTED COPY"
                            : `CONNECTED SERVICE · ${actionLabel(
                                selected.manifest.document.manifest.release,
                              ).toUpperCase()} · AGENTS NEVER SEE THE KEY`}
                        </p>
                        <h2>{selected.envelope.document.metadata.name}</h2>
                        <p className="hint">
                          Version {selected.envelope.document.metadata.version}{" "}
                          ·{" "}
                          {selected.policy.document.expiresAt === null
                            ? "Permission never expires"
                            : `Permission expires ${new Date(
                                selected.policy.document.expiresAt * 1000,
                              ).toLocaleDateString()}`}
                        </p>
                        <div className="button-row">
                          <button
                            className="secondary"
                            disabled={!!busy}
                            onClick={() =>
                              void work("Updating policy…", () =>
                                savePolicy({
                                  disabled: !selected.policy.document.disabled,
                                }),
                              )
                            }
                          >
                            {selected.policy.document.disabled
                              ? "Enable"
                              : "Disable"}
                          </button>
                          <button
                            className="ghost"
                            onClick={() => exportConfig(selected)}
                          >
                            Agent config
                          </button>
                          {selected.manifest.document.manifest.release ===
                            "export" && (
                            <button
                              className="ghost"
                              disabled={
                                !!busy || selected.policy.document.disabled
                              }
                              onClick={reveal}
                            >
                              Export secret
                            </button>
                          )}
                        </div>
                        {isStored(
                          selected.manifest.document.manifest.release,
                        ) && (
                          <details className="docs-details">
                            <summary>How agents use this secret</summary>
                            <p>
                              Hand the value to one command without ever
                              printing it. It reaches only that process's
                              environment, as{" "}
                              <code>
                                {selected.envelope.document.metadata.name}
                              </code>
                              .
                            </p>
                            <pre className="terminal">
                              <code>
                                {`KEYCHAIN_SERVICE_URL=${window.location.origin} ${NPX_KEYCHAIN} run ./agent-identity.json --only ${selected.envelope.document.metadata.name} -- <command>`}
                              </code>
                            </pre>
                            <p>
                              For tools that read credentials from a path, add{" "}
                              <code>
                                --file{" "}
                                {selected.envelope.document.metadata.name}
                                =PATH
                              </code>{" "}
                              to write a private file that is removed when the
                              command exits.
                            </p>
                            <p>
                              In code, <code>keychain.get(name)</code> returns
                              the value; the <code>get_secret</code> MCP tool
                              does the same for MCP clients.
                            </p>
                          </details>
                        )}
                        {!isStored(
                          selected.manifest.document.manifest.release,
                        ) &&
                          ACTIONS[selected.manifest.document.manifest.release]
                            ?.kind === "use" && (
                            <details className="docs-details">
                              <summary>How agents use this service</summary>
                              <ActionDocs
                                compact
                                action={
                                  ACTIONS[
                                    selected.manifest.document.manifest.release
                                  ] as any
                                }
                                secretName={
                                  selected.envelope.document.metadata.name
                                }
                              />
                            </details>
                          )}
                        <hr />
                        <h3>Authorized agents</h3>
                        {selected.policy.document.grants.length === 0 && (
                          <p className="muted">No agents have access.</p>
                        )}
                        {selected.policy.document.grants.map((g) => (
                          <div className="agent-row" key={g.agentPublicKey}>
                            <span>
                              <strong>{g.label}</strong>
                              <code>{brief(g.agentPublicKey)}</code>
                            </span>
                            <span className="row-actions">
                              <button
                                className="ghost"
                                disabled={!!busy}
                                title="Optional legacy static config; live clients discover approvals automatically"
                                onClick={() => void exportAgentConfig(g)}
                              >
                                Download agent config
                              </button>
                              <button
                                className="danger ghost"
                                disabled={!!busy}
                                onClick={() =>
                                  void work("Revoking agent…", () =>
                                    savePolicy({
                                      grants:
                                        selected.policy.document.grants.filter(
                                          (x) =>
                                            x.agentPublicKey !==
                                            g.agentPublicKey,
                                        ),
                                    }),
                                  )
                                }
                              >
                                Revoke
                              </button>
                            </span>
                          </div>
                        ))}
                        <form
                          onSubmit={(e) => {
                            e.preventDefault();
                            const key = agentKey.trim().toLowerCase();
                            const existing =
                              selected.policy.document.grants.find(
                                (g) => g.agentPublicKey === key,
                              );
                            void work("Approving agent…", async () => {
                              setSelected(
                                await client.delegate(selected, key, agentName),
                              );
                              setAgentKey("");
                              setAgentName("");
                              await refresh();
                              setNotice(
                                existing
                                  ? `${brief(key)} was already approved as "${existing.label}"; it is now labelled "${agentName}". Its access did not change.`
                                  : `Approved ${agentName}. Ready to use on its next request with the live SDK, CLI or MCP client. No config download or restart needed.`,
                              );
                            });
                          }}
                        >
                          <label>
                            Agent name
                            <input
                              value={agentName}
                              onChange={(e) => setAgentName(e.target.value)}
                              required
                              maxLength={128}
                              placeholder="Research assistant"
                            />
                          </label>
                          <label>
                            Agent public key
                            <input
                              value={agentKey}
                              onChange={(e) => setAgentKey(e.target.value)}
                              required
                              pattern="[0-9a-fA-F]{64}"
                              title="64 hex characters: the publicKey printed by keychain init (upper- or lowercase)"
                              placeholder="Generate on the agent with keychain init"
                              autoComplete="off"
                            />
                          </label>
                          <button disabled={!!busy}>Approve agent</button>
                          <p className="hint">
                            Approve only a public key you received from your
                            agent. Its private key stays on its device.
                          </p>
                        </form>
                        <details>
                          <summary>Rotate secret</summary>
                          <form
                            onSubmit={(e) => {
                              e.preventDefault();
                              void work(
                                "Encrypting a new version…",
                                async () => {
                                  setSelected(
                                    await client.rotate(selected, rotation),
                                  );
                                  setRotation("");
                                  await refresh();
                                },
                              );
                            }}
                          >
                            <label>
                              New value
                              <textarea
                                value={rotation}
                                onChange={(e) => setRotation(e.target.value)}
                                required
                                autoComplete="off"
                                spellCheck={false}
                              />
                            </label>
                            <p className="hint">
                              Existing agents move to the new version and keep
                              their current expiry. The new policy retires older
                              versions.
                            </p>
                            <button disabled={!!busy}>Rotate & approve</button>
                          </form>
                        </details>
                        <details>
                          <summary>Renew permissions</summary>
                          <label>
                            Days
                            <input
                              type="number"
                              min="1"
                              max={lifetimeCap ?? undefined}
                              value={days}
                              onChange={(e) => setDays(Number(e.target.value))}
                            />
                          </label>
                          <p className="hint">
                            {lifetimeCap === null
                              ? "Any number of days, or remove the expiry so access lasts until you revoke or disable it. Within the lifetime, the operator could replay a revoked policy; a shorter expiry bounds that."
                              : `This secret was created under an earlier release that limits permissions to ${lifetimeCap} days. Recreate it to choose a longer or unlimited lifetime.`}
                          </p>
                          <div className="button-row">
                            <button
                              disabled={!!busy}
                              onClick={() =>
                                void work("Renewing permissions…", () =>
                                  savePolicy({ days }),
                                )
                              }
                            >
                              Renew with owner approval
                            </button>
                            {lifetimeCap === null && (
                              <button
                                className="secondary"
                                disabled={
                                  !!busy ||
                                  selected.policy.document.expiresAt === null
                                }
                                onClick={() =>
                                  void work("Removing expiry…", () =>
                                    savePolicy({ days: null }),
                                  )
                                }
                              >
                                Never expire
                              </button>
                            )}
                          </div>
                        </details>
                        <details>
                          <summary>Verify identity</summary>
                          <p className="hint">Action CID</p>
                          <code className="wrap">
                            {selected.manifest.document.actionCid}
                          </code>
                          <p className="hint">Ciphertext digest</p>
                          <code className="wrap">
                            {digest(selected.envelope.document)}
                          </code>
                        </details>
                      </>
                    ) : (
                      <div className="empty">
                        <h3>Select a secret</h3>
                        <p>
                          Inspect permissions, approve an agent, or rotate its
                          credential.
                        </p>
                      </div>
                    )}
                  </section>
                </div>
              </>
            )}
            {tab === "agents" && (
              <>
                <div className="page-heading">
                  <div>
                    <p className="eyebrow">AGENT ACCESS</p>
                    <h1>Agents</h1>
                    <p>
                      Every approved public key and the secrets it may use.
                      Approve only what each agent needs.
                    </p>
                  </div>
                  <div className="button-row">
                    <button
                      ref={addAgentButton}
                      disabled={!!busy}
                      onClick={() => setAddingAgent({})}
                    >
                      + Add agent
                    </button>
                  </div>
                </div>
                {addingAgent && (
                  <AgentOnboarding
                    client={client}
                    secrets={
                      addingAgent.key
                        ? secrets.filter(
                            (s) =>
                              !((s.agents ?? []) as Grant[]).some(
                                (g) => g.agentPublicKey === addingAgent.key,
                              ),
                          )
                        : secrets
                    }
                    initialName={addingAgent.name}
                    initialKey={addingAgent.key}
                    onBusyChange={(active) =>
                      setBusy(active ? "Approving selected secrets…" : "")
                    }
                    onClose={() => {
                      setAddingAgent(false);
                      addAgentButton.current?.focus();
                    }}
                    onAddSecret={() => {
                      setAddingAgent(false);
                      setTab("secrets");
                      setCreating(true);
                      setSelected(undefined);
                    }}
                    onApproved={async () => {
                      setSelected(undefined);
                      await refresh();
                    }}
                    onDownload={(label, bundles) =>
                      downloadAgentConfig(
                        label,
                        bundles,
                        LIT_URL,
                        client.lit.usageApiKey,
                      )
                    }
                  />
                )}
                <div className="secret-layout" hidden={!!addingAgent}>
                  <section className="secret-list" aria-label="Agents">
                    {agents.length === 0 && (
                      <div className="empty">
                        <h3>No agents yet</h3>
                        <p>
                          {secrets.length === 0
                            ? "Add a secret, then approve an agent’s public key."
                            : "Approve an agent’s public key to give it access to your secrets."}
                        </p>
                      </div>
                    )}
                    {agents.map((a) => (
                      <button
                        className={
                          "secret-row " +
                          (a.key === selectedAgent ? "selected" : "")
                        }
                        key={a.key}
                        onClick={() => setSelectedAgent(a.key)}
                      >
                        <span className="secret-icon agent">◎</span>
                        <span>
                          <strong>{a.label}</strong>
                          <small>
                            <code>{brief(a.key)}</code>
                            {a.labels.length > 1 &&
                              ` · also ${a.labels.slice(1).join(", ")}`}
                          </small>
                        </span>
                        <span className="status">
                          {a.secrets.length === 1
                            ? "1 secret"
                            : a.secrets.length + " secrets"}
                        </span>
                      </button>
                    ))}
                  </section>
                  <section className="detail-card">
                    {agent ? (
                      <>
                        <p className="eyebrow">
                          AGENT · ED25519 PUBLIC KEY · PRIVATE KEY STAYS ON ITS
                          DEVICE
                        </p>
                        <h2>{agent.label}</h2>
                        <code className="wrap">{agent.key}</code>
                        {agent.labels.length > 1 && (
                          <p className="hint">
                            Also approved as {agent.labels.slice(1).join(", ")}{" "}
                            on some secrets.
                          </p>
                        )}
                        <div className="button-row">
                          <button
                            disabled={
                              !!busy || agent.secrets.length >= secrets.length
                            }
                            title={
                              agent.secrets.length >= secrets.length
                                ? "This agent already has access to every secret"
                                : undefined
                            }
                            onClick={() =>
                              setAddingAgent({
                                name: agent.label,
                                key: agent.key,
                              })
                            }
                          >
                            + Grant secrets
                          </button>
                          <button
                            className="ghost"
                            disabled={!!busy}
                            title="Optional legacy static config; live clients discover approvals automatically"
                            onClick={() =>
                              void exportAgentConfig({
                                agentPublicKey: agent.key,
                                label: agent.label,
                              })
                            }
                          >
                            Download agent config
                          </button>
                          <button
                            className="danger ghost"
                            disabled={!!busy}
                            onClick={() => {
                              if (
                                !window.confirm(
                                  `Revoke ${agent.label} from all ${agent.secrets.length} secret${
                                    agent.secrets.length === 1 ? "" : "s"
                                  }?\n\nEach secret needs its own owner approval. Values it already received cannot be recalled.`,
                                )
                              )
                                return;
                              const key = agent.key;
                              const ids = agent.secrets.map((s) => s.secretId);
                              void work("Revoking agent…", async () => {
                                try {
                                  for (const id of ids) {
                                    const bundle = await client.bundle(id);
                                    await client.setPolicy(bundle, {
                                      grants:
                                        bundle.policy.document.grants.filter(
                                          (g) => g.agentPublicKey !== key,
                                        ),
                                    });
                                  }
                                  setSelectedAgent("");
                                  setSelected(undefined);
                                } finally {
                                  await refresh();
                                }
                              });
                            }}
                          >
                            Revoke all
                          </button>
                        </div>
                        <hr />
                        <h3>Accessible secrets</h3>
                        {agent.secrets.map((s) => (
                          <div className="agent-row" key={s.secretId}>
                            <span>
                              <strong>{s.name}</strong>
                              <small className="muted">
                                {" "}
                                · {actionLabel(s.release)} · v{s.version}
                                {s.disabled
                                  ? " · Disabled"
                                  : s.expiresAt !== null &&
                                      s.expiresAt * 1000 <= Date.now()
                                    ? " · Expired"
                                    : s.expiresAt === null
                                      ? " · No expiry"
                                      : ` · Expires ${new Date(
                                          s.expiresAt * 1000,
                                        ).toLocaleDateString()}`}
                              </small>
                            </span>
                            <span className="row-actions">
                              <button
                                className="ghost"
                                disabled={!!busy}
                                onClick={() => {
                                  setTab("secrets");
                                  void pick(s.secretId);
                                }}
                              >
                                Open secret
                              </button>
                              <button
                                className="danger ghost"
                                disabled={!!busy}
                                onClick={() =>
                                  void revokeFromSecret(agent.key, s.secretId)
                                }
                              >
                                Revoke
                              </button>
                            </span>
                          </div>
                        ))}
                        <p className="hint">
                          Revocations apply on the agent's next request. Values
                          it already received cannot be recalled.
                        </p>
                      </>
                    ) : (
                      <div className="empty">
                        <h3>Select an agent</h3>
                        <p>
                          See which secrets it can use, grant more, or revoke
                          access.
                        </p>
                      </div>
                    )}
                  </section>
                </div>
              </>
            )}
            {tab === "recovery" && (
              <>
                <div className="page-heading">
                  <div>
                    <p className="eyebrow">OWNER CONTROL</p>
                    <h1>Recovery & backups</h1>
                    <p>
                      Back up ciphertext and keep another way to approve access.
                    </p>
                  </div>
                </div>
                <section className="detail-card wide">
                  <h2>Back up this vault</h2>
                  <p>
                    One file with everything needed to recover this vault on a
                    new device: your encrypted secrets, their signed policies,
                    and the vault identity. It cannot be read without an
                    approved sign-in method. Download a fresh copy after adding
                    secrets or changing credentials.
                  </p>
                  <button
                    disabled={!!busy}
                    onClick={() =>
                      void work("Verifying and exporting backup…", async () =>
                        download("keychain-backup.json", await client.backup()),
                      )
                    }
                  >
                    Download backup
                  </button>
                  <label>
                    Restore missing secrets
                    <input
                      type="file"
                      accept="application/json"
                      onChange={(e) => {
                        const f = e.target.files?.[0];
                        if (f)
                          void work("Verifying backup…", async () => {
                            await client.restore(await readFile(f));
                            await refresh();
                            setNotice(
                              "Backup restored. Expired policies need owner-approved renewal.",
                            );
                          });
                      }}
                    />
                  </label>
                </section>
                <section className="detail-card wide">
                  <h2>Approved owner credentials</h2>
                  <p>
                    Changes require your current owner’s approval and end
                    existing browser sessions. Download a fresh backup before
                    changing credentials.
                  </p>
                  {recoveryOwners.map((o, i) => (
                    <div className="agent-row" key={digest(o)}>
                      <span>
                        <strong>{o.kind}</strong>
                        <code>
                          {o.kind === "wallet"
                            ? brief(o.address)
                            : o.kind === "google"
                              ? "Google account"
                              : brief(o.credentialId)}
                        </code>
                      </span>
                      <button
                        className="danger ghost"
                        disabled={recoveryOwners.length <= 1 || !!busy}
                        onClick={() =>
                          void work(
                            "Replacing owner credentials…",
                            async () => {
                              await client.updateCredentials(
                                recoveryOwners.filter((_, n) => n !== i),
                              );
                              setClient(undefined);
                              setNotice(
                                "Credentials updated. Sign in again with an approved credential.",
                              );
                            },
                          )
                        }
                      >
                        Remove
                      </button>
                    </div>
                  ))}
                  <form
                    onSubmit={(e) => {
                      e.preventDefault();
                      void work("Adding recovery wallet…", async () => {
                        const owner = ownerSchema.parse({
                          kind: "wallet",
                          address: newWallet.toLowerCase(),
                        });
                        await client.updateCredentials([
                          ...recoveryOwners,
                          owner,
                        ]);
                        setClient(undefined);
                        setNotice("Recovery wallet added. Sign in again.");
                      });
                    }}
                  >
                    <label>
                      Recovery wallet address
                      <input
                        value={newWallet}
                        onChange={(e) => setNewWallet(e.target.value)}
                        required
                        pattern="0x[0-9a-fA-F]{40}"
                      />
                    </label>
                    <button disabled={!!busy}>Approve recovery wallet</button>
                  </form>
                  <button
                    className="secondary"
                    disabled={!!busy}
                    onClick={() =>
                      void work("Creating recovery passkey…", async () => {
                        const identity =
                          await createPasskey("Keychain recovery");
                        await client.updateCredentials([
                          ...recoveryOwners,
                          identity.owner,
                        ]);
                        setClient(undefined);
                        setNotice("Recovery passkey added. Sign in again.");
                      })
                    }
                  >
                    Add recovery passkey
                  </button>
                </section>
              </>
            )}
            {tab === "activity" && (
              <>
                <div className="page-heading">
                  <div>
                    <p className="eyebrow">VAULT HISTORY</p>
                    <h1>Activity</h1>
                    <p>
                      Committed management changes. Direct Lit executions are
                      not a complete access audit.
                    </p>
                  </div>
                </div>
                <section className="detail-card wide">
                  {events.length === 0 && <p>No events yet.</p>}
                  {events.map((e) => (
                    <div className="activity-row" key={e.id}>
                      <strong>{e.event.replaceAll("_", " ")}</strong>
                      <time>{new Date(e.createdAt).toLocaleString()}</time>
                    </div>
                  ))}
                  {events.length >= 100 && (
                    <button
                      className="ghost"
                      onClick={() =>
                        void work("Loading older activity…", async () =>
                          setEvents([
                            ...events,
                            ...(
                              await client.api(
                                `/api/audit?before=${events.at(-1).id}`,
                              )
                            ).events,
                          ]),
                        )
                      }
                    >
                      Load older activity
                    </button>
                  )}
                </section>
              </>
            )}
          </main>
        </div>
      )}
      <LandingFooter
        contactEmail={
          settings?.pricing?.contactEmail ||
          billing?.contactEmail ||
          "support@litprotocol.com"
        }
      />
    </>
  );
}
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <WagmiProvider config={config}>
      <QueryClientProvider client={queryClient}>
        <RainbowKitProvider
          theme={lightTheme({
            accentColor: "#181818",
            accentColorForeground: "#ffffff",
            borderRadius: "small",
            fontStack: "system",
          })}
        >
          <App />
        </RainbowKitProvider>
      </QueryClientProvider>
    </WagmiProvider>
  </React.StrictMode>,
);
