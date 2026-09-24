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
  initialName = "",
  initialKey = "",
}: {
  onBusyChange?: (busy: boolean) => void;
  /** Prefilled from the Agents page to grant an existing agent more secrets. */
  initialName?: string;
  initialKey?: string;
  client: AgentApprovalClient;
  secrets: SecretSummary[];
  onClose: () => void;
  onAddSecret: () => void;
  onApproved: () => Promise<void>;
  onDownload: (label: string, bundles: SecretBundle[]) => void;
}) {
  const [name, setName] = useState(initialName);
  const [key, setKey] = useState(initialKey);
  const existing = !!initialKey;
  const [ids, setIds] = useState<string[]>([]);
  const [approved, setApproved] = useState<SecretBundle[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [complete, setComplete] = useState(false);
  const [downloaded, setDownloaded] = useState(false);
  const running = useRef(false);
  const isEligible = (s: SecretSummary) =>
    !s.disabled && (s.expiresAt === null || s.expiresAt * 1000 > Date.now());
  const eligibleIds = secrets.filter(isEligible).map((s) => s.secretId);
  return (
    <section
      className="detail-card agent-onboarding"
      aria-labelledby="add-agent-title"
    >
      <h2 id="add-agent-title" tabIndex={-1}>
        {existing ? `Grant secrets to ${initialName}` : "Add agent"}
      </h2>
      <p>
        {existing
          ? "Choose more secrets for this agent. It discovers new approvals on its next request."
          : "Already have an agent public key? Add it here, choose what it can use, and it can discover approved secrets on its next request."}
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
        <fieldset disabled={busy || attempted || existing}>
          <legend>1. Identify your agent</legend>
          <label>
            Agent name
            <input
              autoFocus={!existing}
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
          <div className="button-row">
            <button
              type="button"
              className="secondary"
              disabled={
                !eligibleIds.length ||
                eligibleIds.every((id) => ids.includes(id))
              }
              onClick={() =>
                setIds(secrets.filter(isEligible).map((s) => s.secretId))
              }
            >
              Select all
            </button>
            <button
              type="button"
              className="ghost"
              disabled={!ids.length}
              onClick={() => setIds([])}
            >
              Clear selection
            </button>
          </div>
          <p className="hint">
            Select all includes only enabled, unexpired secrets. You can uncheck
            any before approving.
          </p>
          {secrets.length === 0 && (
            <p>
              {existing
                ? "This agent already has access to every secret."
                : "Add a secret before approving an agent."}
            </p>
          )}
          {secrets.map((s) => {
            const unavailable = !isEligible(s);
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
            <h3>3. Ready to use</h3>
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
              No config download or agent restart is needed with the live SDK,
              CLI or MCP client. Your agent keeps its existing private identity
              and connects to this Keychain service. New approvals and
              revocations apply on its next request; already received values
              cannot be recalled.
            </p>
            <pre>
              <code>{`KEYCHAIN_SERVICE_URL=${window.location.origin} keychain list ./agent-identity.json`}</code>
            </pre>
            <details>
              <summary>Advanced: legacy static config</summary>
              <p>
                For older clients only. This snapshot contains scoped billing
                credentials, not secret values. Keep it private. Legacy clients
                need another export to discover newly added secrets.
              </p>
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  try {
                    onDownload(name, approved);
                    setDownloaded(true);
                  } catch (e) {
                    setError(
                      e instanceof Error ? e.message : "Download failed",
                    );
                  }
                }}
              >
                Download agent config
              </button>
              {downloaded && (
                <p>
                  Legacy config downloaded. Keep it and the agent identity
                  private.
                </p>
              )}
            </details>
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
          {secrets.length === 0 && !existing && (
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
            Closing does not undo saved approvals. Use Revoke on the agent's
            page or on each secret's page to remove access.
          </p>
        )}
      </form>
    </section>
  );
}
