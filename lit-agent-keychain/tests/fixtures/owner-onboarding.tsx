import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { AgentOnboarding } from "../../web/src/AgentOnboarding.tsx";
import { downloadAgentConfig } from "../../web/src/agent-config.ts";
import "../../web/src/style.css";
import type { SecretBundle } from "../../sdk/src/index.ts";
const summaries = ["ONE", "TWO", "DISABLED", "EXPIRED"].map((name) => ({
  secretId: name,
  name,
  release: "export",
  disabled: name === "DISABLED",
  expiresAt: name === "EXPIRED" ? 1 : null,
}));
const bundles = Object.fromEntries(
  summaries.map((s) => [
    s.name,
    {
      manifest: {
        document: {
          manifest: { secretId: s.name, release: s.release },
          actionCid: "fixture",
        },
      },
      envelope: { document: { metadata: { name: s.name } } },
      policy: {
        document: { grants: [], disabled: s.disabled, expiresAt: s.expiresAt },
      },
    } as unknown as SecretBundle,
  ]),
);
(window as any).approvals = [];
const client = {
  bundle: async (id: string) => bundles[id],
  delegate: async (b: SecretBundle, key: string, label: string) => {
    const id = b.manifest.document.manifest.secretId;
    if ((window as any).failSecond && id === "TWO")
      throw new Error("Owner declined");
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
};
function Fixture() {
  const [message, setMessage] = useState("");
  return (
    <main style={{ padding: 16 }}>
      {message || (
        <AgentOnboarding
          client={client}
          secrets={location.search.includes("empty") ? [] : summaries}
          onClose={() => setMessage("Closed onboarding")}
          onAddSecret={() => setMessage("Adding secret")}
          onApproved={async () => {}}
          onDownload={(name, approved) =>
            downloadAgentConfig(
              name,
              approved,
              "https://lit.invalid",
              "fixture-execution-key",
            )
          }
        />
      )}
    </main>
  );
}
createRoot(document.getElementById("root")!).render(<Fixture />);
