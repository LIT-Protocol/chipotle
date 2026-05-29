"use client";

import { useCallback, useEffect, useState } from "react";

const short = (a) => (a ? `${a.slice(0, 6)}…${a.slice(-4)}` : "—");
const BASESCAN = "https://sepolia.basescan.org";

function ago(ts) {
  if (!ts) return "";
  const s = Math.max(0, Math.floor(Date.now() / 1000) - ts);
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

export default function Page() {
  const [state, setState] = useState(null);
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const res = await fetch("/api/state", { cache: "no-store" });
      const json = await res.json();
      if (json.error) setError(json.error);
      else {
        setState(json);
        setError(null);
      }
    } catch (e) {
      setError(e.message);
    }
  }, []);

  useEffect(() => {
    load();
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, [load]);

  const toggleKill = async () => {
    if (!state) return;
    setBusy(true);
    try {
      const res = await fetch("/api/killswitch", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ on: !state.killSwitch }),
      });
      const json = await res.json();
      if (json.error) setError(json.error);
      else await load();
    } catch (e) {
      setError(e.message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="wrap">
      <div className="topbar">
        <div className="brand">
          <div className="dot" />
          <div>
            <h1>Lit Solver Vault</h1>
            <div className="sub">
              Policy-gated key custody · Across relayer · Base Sepolia
            </div>
          </div>
        </div>
        <div className="live">
          <span className="pulse" />
          live · refreshes every 5s
        </div>
      </div>

      {error && <div className="err">⚠ {error}</div>}

      {!state && !error && <div className="empty">Loading vault state…</div>}

      {state && (
        <>
          <div className="grid">
            <div className="card">
              <div className="label">Inventory</div>
              <div className="value">
                {Number(state.inventory).toFixed(6)}
                <span className="unit">{state.symbol}</span>
              </div>
              <div className="meta">vault {short(state.vault)}</div>
            </div>

            <div className="card">
              <div className="label">Per-fill cap</div>
              <div className="value">
                {Number(state.maxFillAmount).toFixed(4)}
                <span className="unit">{state.symbol}</span>
              </div>
              <div className="meta">policy: maxFillAmount</div>
            </div>

            <div className="card span-2">
              <div className="label">Policy state</div>
              <div className="killrow">
                <span className={`badge ${state.killSwitch ? "bad" : "ok"}`}>
                  <span
                    className="dot"
                    style={{
                      background: state.killSwitch ? "var(--red)" : "var(--green)",
                      boxShadow: "none",
                    }}
                  />
                  {state.killSwitch ? "KILL SWITCH ENGAGED" : "Operating"}
                </span>
                <button
                  className={`toggle ${state.killSwitch ? "resume" : "danger"}`}
                  onClick={toggleKill}
                  disabled={busy}
                >
                  {busy ? "…" : state.killSwitch ? "Resume fills" : "Engage kill switch"}
                </button>
              </div>
              <div className="meta">signer {short(state.policySigner)} · cold {short(state.coldWallet)}</div>
            </div>
          </div>

          <div className="section-title">Recent fills</div>
          {state.fills.length === 0 ? (
            <div className="empty">
              No fills in the last scanned window. Run <span className="mono">npm run across:fill</span> to land one.
            </div>
          ) : (
            <table>
              <thead>
                <tr>
                  <th>When</th>
                  <th>Deposit</th>
                  <th>Origin</th>
                  <th>Recipient</th>
                  <th>Amount</th>
                  <th>Tx</th>
                </tr>
              </thead>
              <tbody>
                {state.fills.map((f) => (
                  <tr key={f.txHash}>
                    <td>{ago(f.timestamp)}</td>
                    <td className="mono">#{f.depositId}</td>
                    <td className="mono">{f.originChainId}</td>
                    <td className="mono">{short(f.recipient)}</td>
                    <td className="mono">
                      {Number(f.amount).toFixed(6)} {state.symbol}
                    </td>
                    <td>
                      <a href={`${BASESCAN}/tx/${f.txHash}`} target="_blank" rel="noreferrer">
                        {short(f.txHash)}
                      </a>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}

          <div className="footnote">
            Every fill above was released only after the Lit policy action signed it — the relayer
            bot holds no key that can move inventory. Rejected fills never produce a signature, so
            they never reach the chain (and aren&apos;t shown here). Block {state.asOfBlock} · vault{" "}
            <a href={`${BASESCAN}/address/${state.vault}`} target="_blank" rel="noreferrer">
              {short(state.vault)}
            </a>
            .
          </div>
        </>
      )}
    </div>
  );
}
