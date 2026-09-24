import React, { useRef, useState } from "react";
import type { SecretBundle } from "../../sdk/src/index.ts";
import {
  approveAgentSecrets,
  type AgentApprovalClient,
} from "./agent-onboarding.ts";

type SecretSummary = {
  secretId: string;
  name: string;
  release: string;
  disabled: boolean;
  expiresAt: number | null;
};
export function AgentOnboarding({
  client,
  secrets,
  onClose,
  onAddSecret,
  onApproved,
  onDownload,
  onBusyChange,
}: {
  onBusyChange?: (busy: boolean) => void;
  client: AgentApprovalClient;
  secrets: SecretSummary[];
  onClose: () => void;
  onAddSecret: () => void;
  onApproved: () => Promise<void>;
  onDownload: (label: string, bundles: SecretBundle[]) => void;
}) {
  const [name, setName] = useState("");
  const [key, setKey] = useState("");
  const [ids, setIds] = useState<string[]>([]);
  const [approved, setApproved] = useState<SecretBundle[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [complete, setComplete] = useState(false);
  const [downloaded, setDownloaded] = useState(false);
  const running = useRef(false);
  return (
    <section
      className="detail-card agent-onboarding"
      aria-labelledby="add-agent-title"
    >
      <h2 id="add-agent-title" tabIndex={-1}>
        Add agent
      </h2>
      <p>
        Already have an agent public key? Add it here, choose what it can use,
        then give the config file back to your agent.
      </p>
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          if (running.current) return;
          running.current = true;
          setBusy(true);
          onBusyChange?.(true);
          setError("");
          setAttempted(true);
          try {
            const result = await approveAgentSecrets(client, key, name, ids);
            setApproved(result.approved);
            setError(result.error || "");
            setComplete(!result.error);
            try {
              await onApproved();
            } catch {
              setError(
                (result.error ? result.error + " " : "") +
                  "Could not refresh the vault list. Approvals shown here were saved; reload to check your vault.",
              );
            }
          } catch (e) {
            setError(e instanceof Error ? e.message : "Approval failed");
            // Preflight failed before writes; allow the owner to correct their selection.
            if (!approved.length) setAttempted(false);
          } finally {
            running.current = false;
            setBusy(false);
            onBusyChange?.(false);
          }
        }}
      >
        <fieldset disabled={busy || attempted}>
          <legend>1. Identify your agent</legend>
          <label>
            Agent name
            <input
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              maxLength={128}
              placeholder="Research assistant"
            />
          </label>
          <label>
            Agent public key
            <input
              value={key}
              onChange={(e) => setKey(e.target.value)}
              required
              pattern="[0-9a-fA-F]{64}"
              title="64 hex characters, without spaces or a 0x prefix"
              placeholder="Paste the public key from your agent"
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <p className="hint">
            Use only the public key from your agent's existing identity. Never
            upload its identity file or private key. Compare this key with the
            one your agent gave you before approving.
          </p>
        </fieldset>
        <fieldset disabled={busy || attempted}>
          <legend>2. Choose secrets to approve</legend>
          <p className="hint">
            Nothing is selected automatically. Existing agents and permission
            expiry stay unchanged. Disabled or expired secrets must be enabled
            or renewed separately.
          </p>
          {secrets.length === 0 && (
            <p>Add a secret before approving an agent.</p>
          )}
          {secrets.map((s) => {
            const unavailable =
              s.disabled ||
              (s.expiresAt !== null && s.expiresAt * 1000 <= Date.now());
            return (
              <label className="agent-secret-choice" key={s.secretId}>
                <input
                  type="checkbox"
                  checked={ids.includes(s.secretId)}
                  disabled={unavailable}
                  onChange={(e) =>
                    setIds(
                      e.target.checked
                        ? [...ids, s.secretId]
                        : ids.filter((id) => id !== s.secretId),
                    )
                  }
                />
                <span>
                  <strong>{s.name}</strong>
                  <small>
                    {s.release === "export"
                      ? "Agent can read the secret value"
                      : "Agent can use the connected service; cannot read its key"}
                    {s.disabled
                      ? " · Disabled"
                      : unavailable
                        ? " · Expired"
                        : s.expiresAt === null
                          ? " · No expiry"
                          : ` · Expires ${new Date(s.expiresAt * 1000).toLocaleDateString()}`}
                  </small>
                </span>
              </label>
            );
          })}
        </fieldset>
        {!complete && (
          <p className="hint">
            {ids.length} selected. Each secret requires owner approval. If you
            cancel a signing prompt, earlier approvals remain saved.
          </p>
        )}
        {error && (
          <p role="alert" className="onboarding-error">
            {error}
          </p>
        )}
        {busy && <p role="status">Waiting for owner approval…</p>}
        {approved.length > 0 && (
          <div className="agent-handoff" role="status">
            <h3>3. Download agent config</h3>
            <p>
              {approved.length} secret{approved.length === 1 ? "" : "s"} ready
              for this agent.
            </p>
            <ul>
              {approved.map((b) => (
                <li key={b.manifest.document.manifest.secretId}>
                  {b.envelope.document.metadata.name}
                </li>
              ))}
            </ul>
            <p>
              The config contains only these secret locators and a scoped
              execution key, not secret values. Keep it private and send it to
              your agent alongside its existing identity file. Downloading does
              not grant additional access.
            </p>
            <button
              type="button"
              disabled={busy}
              onClick={() => {
                try {
                  onDownload(name, approved);
                  setDownloaded(true);
                } catch (e) {
                  setError(e instanceof Error ? e.message : "Download failed");
                }
              }}
            >
              Download agent config
            </button>
            {downloaded && (
              <p>
                Config downloaded. Give this file to your agent; keep its
                private identity on the agent's device.
              </p>
            )}
          </div>
        )}
        <div className="button-row">
          {!complete && (
            <button
              disabled={busy || !ids.length || !name.trim() || !key}
              type="submit"
            >
              {attempted
                ? "Retry remaining approvals"
                : "Approve selected secrets"}
            </button>
          )}
          {secrets.length === 0 && (
            <button type="button" onClick={onAddSecret}>
              Add secret
            </button>
          )}
          <button
            type="button"
            className="secondary"
            disabled={busy}
            onClick={onClose}
          >
            {attempted ? "Done" : "Cancel"}
          </button>
        </div>
        {attempted && (
          <p className="hint">
            Closing does not undo saved approvals. Use Revoke on each secret's
            page to remove access.
          </p>
        )}
      </form>
    </section>
  );
}
