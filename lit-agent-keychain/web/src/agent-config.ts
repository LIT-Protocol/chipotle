import type { AgentConfig, SecretBundle } from "../../sdk/src/index.ts";

/** Configs contain locators and the existing execution key, never plaintext or identities. */
export function downloadAgentConfig(
  label: string,
  bundles: SecretBundle[],
  litApiUrl: string,
  usageApiKey: string | undefined,
) {
  const secrets: AgentConfig["secrets"] = Object.create(null);
  for (const b of bundles) {
    const name = b.envelope.document.metadata.name;
    if (Object.hasOwn(secrets, name))
      throw new Error(`Duplicate secret name: ${name}`);
    secrets[name] = {
      manifest: b.manifest.document.manifest,
      actionCid: b.manifest.document.actionCid,
    };
  }
  const config: AgentConfig = { v: 2, litApiUrl, usageApiKey, secrets };
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(config, null, 2)], { type: "application/json" }),
  );
  const a = document.createElement("a");
  a.href = url;
  a.download = `${label.replace(/[^A-Za-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "") || "agent"}.keychain.json`;
  a.click();
  URL.revokeObjectURL(url);
}
