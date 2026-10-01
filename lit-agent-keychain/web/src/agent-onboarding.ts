import type { OwnerClient, SecretBundle } from "../../sdk/src/index.ts";

export type AgentApprovalClient = Pick<OwnerClient, "bundle" | "delegate">;

/** Preflight the explicit selection before asking for any owner signatures.
 * Delegate preserves all other grants, disabled state and the current expiry.
 * Multi-secret approvals are separate transactions, never an atomic batch.
 */
export async function approveAgentSecrets(
  client: AgentApprovalClient,
  publicKey: string,
  label: string,
  secretIds: string[],
): Promise<{ approved: SecretBundle[]; error?: string }> {
  if (!/^[0-9a-fA-F]{64}$/.test(publicKey))
    throw new Error("Use only the agent public key: 64 hex characters.");
  if (!label.trim() || label.length > 128)
    throw new Error("Enter an agent name of 1–128 characters.");
  if (!secretIds.length) throw new Error("Select at least one secret.");
  const key = publicKey.toLowerCase();
  const bundles = [];
  for (const id of new Set(secretIds)) {
    const b = await client.bundle(id);
    const p = b.policy.document;
    if (
      p.disabled ||
      (p.expiresAt !== null && p.expiresAt <= Date.now() / 1000)
    )
      throw new Error(
        `${b.envelope.document.metadata.name} is disabled or expired. Enable or renew it on its secret page first.`,
      );
    bundles.push(b);
  }
  const approved: SecretBundle[] = [];
  for (const b of bundles) {
    try {
      // Retrying a partial batch must not silently renew or relabel prior grants.
      approved.push(
        b.policy.document.grants.some((g) => g.agentPublicKey === key)
          ? b
          : await client.delegate(b, key, label.trim(), {
              preserveExpiry: true,
            }),
      );
    } catch (e) {
      return {
        approved,
        error: `${b.envelope.document.metadata.name}: ${e instanceof Error ? e.message : "Approval failed"}. This approval could not be confirmed; earlier approvals remain saved. Later secrets were not attempted. Retry to check current access before continuing.`,
      };
    }
  }
  return { approved };
}
