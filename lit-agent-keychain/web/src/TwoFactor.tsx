import React, { useEffect, useRef, useState } from "react";
import QRCode from "qrcode";
import { OwnerClient } from "../../sdk/src/index.ts";
import { HttpError } from "../../protocol/client-http.ts";

export function twoFactorError(error: unknown): string {
  if (error instanceof HttpError) {
    if (error.status === 429)
      return "Too many attempts. Wait five minutes before trying again.";
    if (error.detail.includes("invalid_or_used_two_factor_code"))
      return "That code is incorrect or already used. Try the next code from your app, or an unused recovery code.";
    if (error.detail.includes("two_factor_login_expired"))
      return "This sign-in expired. Cancel and sign in again.";
    if (error.detail.includes("two_factor_setup_expired"))
      return "Setup expired. Cancel setup and start again.";
  }
  return error instanceof Error
    ? error.message
    : "Could not verify your code. Try again.";
}
export type TwoFactorPrompt = {
  verify: (code: string) => Promise<void>;
  cancel: () => void;
};
export function TwoFactorLogin({ prompt }: { prompt: TwoFactorPrompt }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [code, setCode] = useState("");
  const [recovery, setRecovery] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  return (
    <dialog
      ref={dialog}
      className="two-factor-dialog"
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) prompt.cancel();
      }}
      aria-labelledby="two-factor-title"
    >
      <h2 id="two-factor-title">Two-factor authentication</h2>
      <p>
        {recovery
          ? "Enter one of the recovery codes you saved when setting up 2FA. Each code works once."
          : "Enter the six-digit code from your authenticator app to finish signing in."}
      </p>
      {error && (
        <p role="alert" className="alert error">
          {error}
        </p>
      )}
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          if (busy) return;
          setBusy(true);
          setError("");
          try {
            await prompt.verify(code.trim());
          } catch (e) {
            setError(twoFactorError(e));
            setCode("");
            setBusy(false);
          }
        }}
      >
        <label>
          {recovery ? "Recovery code" : "Authenticator code"}
          <input
            autoFocus
            value={code}
            onChange={(e) => setCode(e.target.value)}
            autoComplete="one-time-code"
            inputMode={recovery ? "text" : "numeric"}
            pattern={recovery ? undefined : "[0-9]{6}"}
            maxLength={recovery ? 64 : 6}
            required
            disabled={busy}
          />
        </label>
        <div className="row-actions">
          <button disabled={busy}>
            {busy ? "Verifying…" : "Verify & sign in"}
          </button>
          <button
            type="button"
            className="ghost"
            disabled={busy}
            onClick={prompt.cancel}
          >
            Cancel
          </button>
        </div>
      </form>
      <button
        className="ghost"
        disabled={busy}
        onClick={() => {
          setRecovery(!recovery);
          setCode("");
          setError("");
        }}
      >
        {recovery ? "Use an authenticator code" : "Use a recovery code"}
      </button>
    </dialog>
  );
}

type Props = {
  client: OwnerClient;
  busy: boolean;
  work: (label: string, operation: () => Promise<void>) => Promise<void>;
};
export function TwoFactorSettings({ client, busy, work }: Props) {
  const [status, setStatus] = useState<{
    enabled: boolean;
    recoveryCodesRemaining: number;
  }>();
  const [loadError, setLoadError] = useState("");
  const [setup, setSetup] = useState<{ secret: string; qr: string }>();
  const [codes, setCodes] = useState<string[]>();
  const [saved, setSaved] = useState(false);
  const [code, setCode] = useState("");
  const [action, setAction] = useState<"disable" | "regenerate">();
  const load = () =>
    client
      .securityStatus()
      .then(setStatus)
      .catch((e) => setLoadError(twoFactorError(e)));
  useEffect(() => {
    void load();
  }, [client]);
  const run = (label: string, operation: () => Promise<void>) =>
    work(label, async () => {
      try {
        await operation();
      } catch (e) {
        if (
          e instanceof HttpError &&
          (e.status === 429 || e.detail.includes("two_factor_"))
        )
          throw new Error(twoFactorError(e));
        throw e;
      }
    });
  return (
    <section
      className="detail-card wide two-factor-card"
      aria-labelledby="totp-heading"
    >
      <div className="two-factor-heading">
        <h2 id="totp-heading">Two-factor authentication</h2>
        {status && (
          <span
            className={`two-factor-status${status.enabled ? " is-enabled" : ""}`}
          >
            <span className="two-factor-status-dot" aria-hidden="true" />
            {status.enabled
              ? "Enabled"
              : setup
                ? "Setup in progress"
                : "Not enabled"}
          </span>
        )}
      </div>
      <p className="two-factor-description">
        Require a code from your authenticator app every time you sign in, with
        any of your approved sign-in methods.
      </p>
      {loadError && (
        <p role="alert">
          {loadError}{" "}
          <button
            className="ghost"
            onClick={() => {
              setLoadError("");
              void load();
            }}
          >
            Retry
          </button>
        </p>
      )}
      {!status && !loadError && <p role="status">Loading security settings…</p>}
      {status && (
        <div className="two-factor-content">
          {!status.enabled && !setup && (
            <button
              disabled={busy}
              onClick={() =>
                void run("Approving authenticator setup…", async () => {
                  const result = await client.setupTwoFactor();
                  const qr = await QRCode.toDataURL(result.uri, {
                    width: 256,
                    margin: 2,
                  });
                  setSetup({ secret: result.secret, qr });
                  setCode("");
                })
              }
            >
              Set up authenticator app
            </button>
          )}
          {setup && (
            <>
              <h3>Connect your authenticator</h3>
              <p>
                Scan this QR code in Google Authenticator, 1Password, or another
                authenticator app. Setup expires after 10 minutes.
              </p>
              <img
                className="totp-qr"
                src={setup.qr}
                alt="Scan with your authenticator app"
                width={256}
                height={256}
              />
              <details>
                <summary>Can’t scan the code?</summary>
                <p>
                  Choose “Enter a setup key” in your app, use this key, and
                  select time-based codes.
                </p>
                <code className="totp-secret">{setup.secret}</code>
              </details>
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void run("Verifying authenticator…", async () => {
                    const result = await client.confirmTwoFactor(code.trim());
                    setSetup(undefined);
                    setCode("");
                    setCodes(result.recoveryCodes);
                    setSaved(false);
                    setStatus({
                      enabled: true,
                      recoveryCodesRemaining: result.recoveryCodes.length,
                    });
                  });
                }}
              >
                <label>
                  Authenticator code
                  <input
                    value={code}
                    onChange={(e) => setCode(e.target.value)}
                    autoComplete="one-time-code"
                    inputMode="numeric"
                    pattern="[0-9]{6}"
                    maxLength={6}
                    required
                    disabled={busy}
                  />
                </label>
                <div className="row-actions">
                  <button disabled={busy}>Verify & enable 2FA</button>
                  <button
                    type="button"
                    className="ghost"
                    disabled={busy}
                    onClick={() => {
                      setSetup(undefined);
                      setCode("");
                    }}
                  >
                    Cancel setup
                  </button>
                </div>
              </form>
              <p className="hint">
                2FA stays off until you verify a code. Enabling it signs out
                your other browser sessions.
              </p>
            </>
          )}
          {codes && (
            <div
              className="recovery-codes"
              role="region"
              aria-label="Save recovery codes"
            >
              <h3>Save your recovery codes</h3>
              <p>
                These codes are shown only now. Store them somewhere safe
                outside this vault. Each replaces an authenticator code once;
                you still need an approved sign-in method.
              </p>
              <ul>
                {codes.map((value) => (
                  <li key={value}>
                    <code>{value}</code>
                  </li>
                ))}
              </ul>
              <button
                className="secondary"
                onClick={() => {
                  const blob = new Blob(
                    [
                      `Lit Keychain recovery codes\nVault: ${client.vaultId}\nEach code works once. Keep this file private.\n\n${codes.join("\n")}\n`,
                    ],
                    { type: "text/plain" },
                  );
                  const url = URL.createObjectURL(blob);
                  const link = document.createElement("a");
                  link.href = url;
                  link.download = "keychain-recovery-codes.txt";
                  link.click();
                  setTimeout(() => URL.revokeObjectURL(url), 1000);
                }}
              >
                Download recovery codes
              </button>
              <label className="check">
                <input
                  type="checkbox"
                  checked={saved}
                  onChange={(e) => setSaved(e.target.checked)}
                />
                I saved my recovery codes
              </label>
              <button
                disabled={!saved}
                onClick={() => {
                  setCodes(undefined);
                  setSaved(false);
                }}
              >
                Done
              </button>
            </div>
          )}
          {status.enabled && !codes && (
            <>
              <p>
                {status.recoveryCodesRemaining} recovery codes remaining. A
                recovery sign-in method still requires an authenticator code or
                one of these codes.
              </p>
              {status.recoveryCodesRemaining === 0 && (
                <p role="alert">
                  You have no recovery codes left. Generate a new set while you
                  can access your authenticator.
                </p>
              )}
              {!action ? (
                <div className="row-actions">
                  <button
                    className="secondary"
                    disabled={busy}
                    onClick={() => {
                      setAction("regenerate");
                      setCode("");
                    }}
                  >
                    Generate new recovery codes
                  </button>
                  <button
                    className="danger ghost"
                    disabled={busy}
                    onClick={() => {
                      setAction("disable");
                      setCode("");
                    }}
                  >
                    Turn off 2FA
                  </button>
                </div>
              ) : (
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    void run("Approving security change…", async () => {
                      if (action === "disable") {
                        await client.disableTwoFactor(code.trim());
                        setStatus({
                          enabled: false,
                          recoveryCodesRemaining: 0,
                        });
                      } else {
                        const result = await client.regenerateRecoveryCodes(
                          code.trim(),
                        );
                        setCodes(result.recoveryCodes);
                        setSaved(false);
                        setStatus({
                          enabled: true,
                          recoveryCodesRemaining: result.recoveryCodes.length,
                        });
                      }
                      setAction(undefined);
                      setCode("");
                    });
                  }}
                >
                  <h3>
                    {action === "disable"
                      ? "Turn off two-factor authentication?"
                      : "Replace recovery codes?"}
                  </h3>
                  <p>
                    {action === "disable"
                      ? "Future sign-ins will only require one approved sign-in method."
                      : "All previous recovery codes will stop working. Save the new set before leaving this page."}{" "}
                    Your other browser sessions will be signed out.
                  </p>
                  <label>
                    Authenticator or recovery code
                    <input
                      value={code}
                      onChange={(e) => setCode(e.target.value)}
                      autoComplete="one-time-code"
                      maxLength={64}
                      required
                      disabled={busy}
                    />
                  </label>
                  <div className="row-actions">
                    <button disabled={busy}>
                      {action === "disable"
                        ? "Confirm turn off 2FA"
                        : "Replace recovery codes"}
                    </button>
                    <button
                      className="ghost"
                      type="button"
                      disabled={busy}
                      onClick={() => {
                        setAction(undefined);
                        setCode("");
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                </form>
              )}
            </>
          )}
        </div>
      )}
      {status && (
        <p className="hint two-factor-note">
          This protects Keychain sign-in. Your approved agents continue working.
          Authenticator keys and recovery codes are not included in vault
          backups.
        </p>
      )}
    </section>
  );
}
