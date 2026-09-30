import React, { useId, useState } from "react";
import { version } from "../../sdk/package.json";

const identityPath = "/absolute/path/agent-identity.json";
const shellQuote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`;

export function connectionSnippets(
  name: string,
  publicKey: string,
  serviceUrl: string,
) {
  const pkg = `@lit-protocol/keychain@${version}`;
  const args = ["-y", pkg, "mcp", identityPath];
  return [
    {
      label: "Agent prompt",
      text: `Use Lit Agent Keychain as the agent named ${JSON.stringify(name)} (public key ${publicKey}). Follow https://keychain.litprotocol.com/SKILL.md. Use your existing local identity file and verify its public key matches ${publicKey}; if it does not match, stop and locate the matching identity. Keep the identity file and private key local. Use Keychain service ${serviceUrl}. Run KEYCHAIN_SERVICE_URL=${shellQuote(serviceUrl)} npx -y ${pkg} list /path/to/your/existing/agent-identity.json to discover your current approved secrets. Do not create a replacement identity or request a config download.`,
    },
    {
      label: "Claude Code",
      text: `claude mcp add --env KEYCHAIN_SERVICE_URL=${shellQuote(serviceUrl)} --transport stdio lit-keychain -- npx ${args.join(" ")}`,
    },
    {
      label: "Codex",
      text: `codex mcp add lit-keychain --env KEYCHAIN_SERVICE_URL=${shellQuote(serviceUrl)} -- npx ${args.join(" ")}`,
    },
    {
      label: "Cursor / Windsurf",
      text: JSON.stringify(
        {
          mcpServers: {
            "lit-keychain": {
              command: "npx",
              args,
              env: { KEYCHAIN_SERVICE_URL: serviceUrl },
            },
          },
        },
        null,
        2,
      ),
    },
    {
      label: "SDK",
      text: `// Install: npm install ${pkg}\nimport { readFile } from "node:fs/promises";\nimport { LiveKeychain } from "@lit-protocol/keychain";\n\nconst identity = JSON.parse(await readFile(${JSON.stringify(identityPath)}, "utf8"));\nif (identity.publicKey !== ${JSON.stringify(publicKey)}) {\n  throw new Error("Identity public key does not match the approved agent");\n}\nconst keychain = new LiveKeychain(identity.privateKey, {\n  serviceUrl: ${JSON.stringify(serviceUrl)},\n});\nconsole.log(await keychain.list());`,
    },
  ];
}

export function AgentConnection({
  name,
  publicKey,
}: {
  name: string;
  publicKey: string;
}) {
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState("");
  const id = useId();
  const snippets = connectionSnippets(name, publicKey, window.location.origin);
  return (
    <div className="agent-connection">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen(!open)}
      >
        Connect to a session
      </button>
      {open && (
        <section id={id} aria-label={`Connect ${name} to a session`}>
          <p>
            Paste the agent prompt into the session you want to connect. The
            session must have the existing identity whose public key matches{" "}
            <code className="wrap">{publicKey}</code>.
          </p>
          <p>
            For MCP or SDK setup, replace <code>{identityPath}</code> with the
            path on the agent’s machine. Keep the identity file there. After
            connecting MCP, use <code>agent_public_key</code> to verify the key
            above, then <code>list_secrets</code> to see current approvals. A
            name alone does not connect a session.
          </p>
          {snippets.map(({ label, text }) => (
            <div key={label}>
              <h4>{label}</h4>
              <button
                type="button"
                className="secondary"
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(text);
                    setStatus(`Copied ${label}`);
                  } catch {
                    setStatus(
                      `Could not copy ${label}. Select and copy the text below.`,
                    );
                  }
                }}
              >
                Copy {label}
              </button>
              <pre>
                <code>{text}</code>
              </pre>
            </div>
          ))}
          <p role="status">{status}</p>
        </section>
      )}
    </div>
  );
}
