import React, { useState, useEffect, useRef, useCallback } from "react";
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
  darkTheme,
  connectorsForWallets,
} from "@rainbow-me/rainbowkit";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  OwnerClient,
  LitConnection,
  Keychain,
  digest,
  type Authority,
  type Grant,
  type SecretBundle,
  type AgentConfig,
} from "../../sdk/src/index.ts";
import { jsonFetch } from "../../protocol/http.ts";
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
        width: 280,
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
  const [tab, setTab] = useState<"secrets" | "recovery" | "activity">(
    "secrets",
  );
  const [events, setEvents] = useState<any[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [release, setRelease] = useState<"export" | "stripe_balance">("export");
  const [agentKey, setAgentKey] = useState("");
  const [agentName, setAgentName] = useState("");
  const [rotation, setRotation] = useState("");
  const [days, setDays] = useState(30);
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
      setSelected(await client!.bundle(id));
      setCreating(false);
      setRotation("");
    });
  const savePolicy = async (changes: {
    grants?: Grant[];
    disabled?: boolean;
    days?: number;
  }) => {
    const updated = await client!.setPolicy(selected!, changes);
    setSelected(updated);
    await refresh();
  };
  const exportConfig = (bundle: SecretBundle) => {
    const m = bundle.manifest.document.manifest;
    const config: AgentConfig = {
      v: 2,
      litApiUrl: LIT_URL,
      usageApiKey: client!.lit.usageApiKey,
      secrets: {
        [bundle.envelope.document.metadata.name]: {
          manifest: m,
          actionCid: bundle.manifest.document.actionCid,
        },
      },
    };
    download(`${bundle.envelope.document.metadata.name}.keychain.json`, config);
  };
  const reveal = () =>
    work("Authorizing a temporary reader…", async () => {
      if (!selected) return;
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
  return (
    <>
      <header>
        <a href="/" className="brand">
          <span className="brand-mark">L</span> Lit <span>Agent Keychain</span>
        </a>
        <div className="header-right">
          <span className="pill">Encrypted locally</span>
          {client && (
            <button
              className="ghost"
              onClick={() =>
                work("Signing out…", async () => {
                  await fetch("/auth/logout", { method: "POST" });
                  client.lit.usageApiKey = undefined;
                  setClient(undefined);
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
        <main className="welcome">
          <section>
            <p className="eyebrow">CREDENTIALS, UNDER YOUR CONTROL</p>
            <h1>
              Your agents.
              <br />
              Your keys.
              <br />
              <em>Your permission.</em>
            </h1>
            <p className="intro">
              Encrypt secrets on your device. Decide which agents can use them.
              Lit Actions check your authorization before every release.
            </p>
            <div className="trust-note">
              <span>01</span>
              <div>
                <strong>Owner-approved access</strong>
                <p>
                  Wallet, passkey, or Google. Keychain cannot invent
                  permissions.
                </p>
              </div>
            </div>
            <div className="trust-note">
              <span>02</span>
              <div>
                <strong>Encrypted storage</strong>
                <p>
                  Only ciphertext reaches our database. Agents prove they hold
                  their own key.
                </p>
              </div>
            </div>
            <div className="trust-note">
              <span>03</span>
              <div>
                <strong>A clear trust boundary</strong>
                <p>
                  You trust the client, Lit, your sign-in provider, and Keychain
                  to honor the latest revocations.
                </p>
              </div>
            </div>
            <section className="pricing-card" aria-label="Pricing">
              <p className="eyebrow">SIMPLE PRICING</p>
              <h2>
                $10 <small>/ month</small>
              </h2>
              <p>
                Up to 1,000 secrets per account. All sign-in methods, agent
                access, rotation, and recovery included.
              </p>
              <p>
                Execution included under fair use. No automatic overage charges.
                Rotations do not use extra secret slots.
              </p>
              <a
                href={`mailto:${encodeURIComponent(settings?.pricing?.contactEmail || "support@litprotocol.com")}?subject=Keychain%20custom%20plan`}
              >
                More secrets or high-volume usage? Contact us
              </a>
            </section>
          </section>
          <section className="login-card">
            <p className="eyebrow">GET STARTED</p>
            <h2>Choose how you sign in</h2>
            <p>
              Each method can own a vault. Google requires no wallet or passkey.
            </p>
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
            <div className="divider">or use a passkey</div>
            <button
              className="secondary full"
              disabled={!!busy || !settings}
              onClick={() =>
                work("Creating a passkey…", async () => {
                  const identity = await createPasskey("My Keychain");
                  const c = ownerClient(identity, settings.network, recovery);
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
              className="ghost full"
              disabled={!!busy || !settings}
              onClick={() =>
                work("Finding your passkey…", async () => {
                  const found = await discoverPasskey();
                  const c = ownerClient(
                    found.identity,
                    settings.network,
                    found.authority || recovery,
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
            <details>
              <summary>Recover an existing vault</summary>
              <p>
                Load its encrypted backup or recovery descriptor, then sign in
                with an approved recovery credential.
              </p>
              <input
                aria-label="Recovery file"
                type="file"
                accept="application/json"
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f)
                    void work("Loading recovery descriptor…", async () => {
                      const data = await readFile(f);
                      await OwnerClient.restoreCredentials(
                        data,
                        new LitConnection(LIT_URL),
                      );
                      setRecovery(data.authority);
                      setNotice(
                        "Recovery vault selected. Sign in with one of its approved credentials.",
                      );
                    });
                }}
              />
              {recovery && <p>Vault {brief(digest(recovery))} selected.</p>}
            </details>
          </section>
        </main>
      ) : (
        <div className="workspace">
          <aside>
            <p className="eyebrow">YOUR VAULT</p>
            <code>{brief(client.vaultId)}</code>
            <nav>
              {(["secrets", "recovery", "activity"] as const).map((t) => (
                <button
                  key={t}
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
                    : t === "recovery"
                      ? "Recovery & backups"
                      : "Activity"}
                </button>
              ))}
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
                    : "$10/month · Standard"}
                </strong>
                <p>
                  {secrets.length.toLocaleString()} /{" "}
                  {(
                    billing?.subscription?.secretLimit ?? 1000
                  ).toLocaleString()}{" "}
                  secrets
                </p>
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
                    Subscribe to add and use secrets. Your encrypted backups
                    remain available.
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
              <details>
                <summary>Execution and account access</summary>
                <p>
                  Execution is included under fair use, with no automatic
                  overage charges. Canceled subscriptions remain active through
                  the paid period. Cancellation never deletes your secrets or
                  encrypted backups.
                </p>
                <p>
                  Agent configurations include a scoped execution key. Replacing
                  it stops old configurations from connecting; agents still need
                  your separate approval to access secrets.
                </p>
                <button
                  className="secondary"
                  disabled={!!busy}
                  onClick={() =>
                    work("Replacing execution key…", async () => {
                      const { usageApiKey } = await client.api(
                        "/api/execution-key/rotate",
                        { method: "POST" },
                      );
                      client.lit.usageApiKey = usageApiKey;
                      setNotice(
                        "Execution key replaced. Download updated configurations for your agents.",
                      );
                    })
                  }
                >
                  Replace execution key
                </button>
              </details>
            </section>
            {tab === "secrets" && (
              <>
                <div className="page-heading">
                  <div>
                    <p className="eyebrow">AGENT ACCESS</p>
                    <h1>Secrets</h1>
                    <p>
                      Start with no agent access. Grant only what each agent
                      needs.
                    </p>
                  </div>
                  <button
                    disabled={
                      !!busy ||
                      !billing?.subscription?.active ||
                      secrets.length >= billing.subscription.secretLimit
                    }
                    onClick={() => {
                      setCreating(true);
                      setSelected(undefined);
                    }}
                  >
                    + Add secret
                  </button>
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
                    {secrets.map((s) => (
                      <button
                        className={
                          "secret-row " +
                          (selected?.manifest.document.manifest.secretId ===
                          s.secretId
                            ? "selected"
                            : "")
                        }
                        key={s.secretId}
                        onClick={() => pick(s.secretId)}
                      >
                        <span className="secret-icon">⌘</span>
                        <span>
                          <strong>{s.name}</strong>
                          <small>
                            {s.release === "export"
                              ? "Encrypted release"
                              : "Use in Lit only"}{" "}
                            · v{s.version}
                          </small>
                        </span>
                        <span className={"status " + (s.disabled ? "off" : "")}>
                          {s.disabled
                            ? "Disabled"
                            : s.expiresAt * 1000 <= Date.now()
                              ? "Expired"
                              : s.agentCount + " agents"}
                        </span>
                      </button>
                    ))}
                  </section>
                  <section className="detail-card">
                    {creating ? (
                      <form
                        onSubmit={(e) => {
                          e.preventDefault();
                          void work(
                            "Encrypting and approving secret…",
                            async () => {
                              const bundle = await client.create(
                                name,
                                value,
                                release,
                              );
                              setValue("");
                              setName("");
                              setCreating(false);
                              setSelected(bundle);
                              await refresh();
                              setNotice(
                                "Secret saved. No agents have access yet.",
                              );
                            },
                          );
                        }}
                      >
                        <h2>Add a secret</h2>
                        <label>
                          Name
                          <input
                            required
                            pattern="[A-Z][A-Z0-9_]{0,63}"
                            value={name}
                            onChange={(e) => setName(e.target.value)}
                            placeholder="STRIPE_API_KEY"
                            autoComplete="off"
                          />
                        </label>
                        <label>
                          Secret value
                          <textarea
                            required
                            value={value}
                            onChange={(e) => setValue(e.target.value)}
                            autoComplete="off"
                            spellCheck={false}
                            placeholder="Encrypted before upload"
                          />
                        </label>
                        <label>
                          How agents may use it
                          <select
                            value={release}
                            onChange={(e) => setRelease(e.target.value as any)}
                          >
                            <option value="export">
                              Receive an encrypted copy
                            </option>
                            <option value="stripe_balance">
                              Read Stripe balance inside Lit
                            </option>
                          </select>
                        </label>
                        <p className="hint">
                          {release === "stripe_balance"
                            ? "This action cannot export the credential, even for recovery. Keep your original credential elsewhere."
                            : "Only an authorized agent can decrypt its response locally."}
                        </p>
                        <button disabled={!!busy}>Encrypt & save</button>
                        <button
                          type="button"
                          className="ghost"
                          onClick={() => {
                            setCreating(false);
                            setValue("");
                          }}
                        >
                          Cancel
                        </button>
                      </form>
                    ) : selected ? (
                      <>
                        <p className="eyebrow">
                          {selected.manifest.document.manifest.release ===
                          "export"
                            ? "ENCRYPTED RELEASE"
                            : "STRICT USE WITHOUT REVEAL"}
                        </p>
                        <h2>{selected.envelope.document.metadata.name}</h2>
                        <p className="hint">
                          Version {selected.envelope.document.metadata.version}{" "}
                          · Permission expires{" "}
                          {new Date(
                            selected.policy.document.expiresAt * 1000,
                          ).toLocaleDateString()}
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
                            <button
                              className="danger ghost"
                              disabled={!!busy}
                              onClick={() =>
                                void work("Revoking agent…", () =>
                                  savePolicy({
                                    grants:
                                      selected.policy.document.grants.filter(
                                        (x) =>
                                          x.agentPublicKey !== g.agentPublicKey,
                                      ),
                                  }),
                                )
                              }
                            >
                              Revoke
                            </button>
                          </div>
                        ))}
                        <form
                          onSubmit={(e) => {
                            e.preventDefault();
                            void work("Approving agent…", async () => {
                              setSelected(
                                await client.delegate(
                                  selected,
                                  agentKey,
                                  agentName,
                                ),
                              );
                              setAgentKey("");
                              setAgentName("");
                              await refresh();
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
                              pattern="[0-9a-f]{64}"
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
                              Existing agents move to the new version. The new
                              policy retires older versions.
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
                              max="90"
                              value={days}
                              onChange={(e) => setDays(Number(e.target.value))}
                            />
                          </label>
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
                  <h2>Encrypted backup</h2>
                  <p>
                    Includes current secret versions, signed policies, and the
                    vault descriptor. Store it somewhere you control. You still
                    need an approved sign-in method.
                  </p>
                  <button
                    disabled={!!busy}
                    onClick={() =>
                      void work("Verifying and exporting backup…", async () =>
                        download(
                          "keychain-encrypted-backup.json",
                          await client.backup(),
                        ),
                      )
                    }
                  >
                    Download encrypted backup
                  </button>
                  <button
                    className="ghost"
                    onClick={() =>
                      download("keychain-recovery.json", {
                        v: 2,
                        authority: client.authority,
                      })
                    }
                  >
                    Recovery descriptor
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
                    existing browser sessions. Save your recovery descriptor
                    before changing credentials.
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
                                "Credentials updated. Sign in with an approved credential and your recovery descriptor.",
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
                        setNotice(
                          "Recovery passkey added. Use your recovery descriptor to sign in.",
                        );
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
      <footer>
        Open source client · Lit Action authorization ·{" "}
        <span>
          Revocation relies on Keychain serving the latest signed policy.
        </span>
      </footer>
    </>
  );
}
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <WagmiProvider config={config}>
      <QueryClientProvider client={queryClient}>
        <RainbowKitProvider
          theme={darkTheme({
            accentColor: "#bded81",
            accentColorForeground: "#152017",
            borderRadius: "medium",
          })}
        >
          <App />
        </RainbowKitProvider>
      </QueryClientProvider>
    </WagmiProvider>
  </React.StrictMode>,
);
