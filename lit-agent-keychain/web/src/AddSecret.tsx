// The "Add" flow. A stored secret (agents receive an encrypted copy) and a
// connected service (agents run a reviewed catalog action; the credential never
// leaves the enclave) are different products, so the first step asks which one
// the owner wants. Connected services then get a catalog picker and a per-action
// wizard rendered from the action manifest: what it does, what the agent calls,
// what comes back, and what it can never do. The choice is permanent: the release
// id is part of the secret's encryption key.
import React, { useMemo, useState } from "react";
import {
  availableActions,
  type ActionDefinition,
} from "../../sdk/src/index.ts";

type Shape = NonNullable<Extract<ActionDefinition, { kind: "use" }>["input"]>;
type UseDefinition = Extract<ActionDefinition, { kind: "use" }>;

/** Where to obtain a credential for the verified first-party actions. */
const CREDENTIAL_SOURCES: Record<string, { label: string; url: string }> = {
  stripe_balance: {
    label: "Stripe dashboard → Developers → API keys",
    url: "https://dashboard.stripe.com/apikeys",
  },
  openai_chat: {
    label: "OpenAI platform → API keys",
    url: "https://platform.openai.com/api-keys",
  },
  github_read_file: {
    label: "GitHub → Settings → Developer settings → Tokens",
    url: "https://github.com/settings/tokens",
  },
  slack_post_message: {
    label: "Slack API → Your apps → OAuth & Permissions",
    url: "https://api.slack.com/apps",
  },
};

const CATEGORY_LABELS: Record<string, string> = {
  payments: "Payments",
  ai: "AI",
  developer: "Developer",
  messaging: "Messaging",
  data: "Data",
  other: "Other",
};

/** A plausible example value for a shape, used to render the agent-side snippet. */
function exampleFor(shape: Shape, key = ""): unknown {
  switch (shape.type) {
    case "object": {
      const out: Record<string, unknown> = {};
      const required = new Set(shape.required ?? []);
      for (const [k, v] of Object.entries(shape.properties)) {
        if (required.size === 0 || required.has(k)) out[k] = exampleFor(v, k);
      }
      return out;
    }
    case "array":
      return [exampleFor(shape.items, key)];
    case "string": {
      if (shape.enum?.length)
        return shape.enum.includes("user") ? "user" : shape.enum[0];
      // "for example gpt-4o-mini" in a description becomes the example value.
      const hinted = shape.description?.match(
        /(?:for example|e\.g\.)\s+([^\s,;.]+)/i,
      );
      return hinted ? hinted[1] : `<${key || "text"}>`;
    }
    case "integer":
      return shape.minimum ?? 1;
    case "number":
      return 1;
    case "boolean":
      return true;
  }
}

/** Flattens a shape into "field: type — description" rows for the docs table. */
function fieldRows(
  shape: Shape,
  prefix = "",
  required = true,
): { path: string; type: string; description?: string; required: boolean }[] {
  if (shape.type === "object") {
    const req = new Set(shape.required ?? []);
    return Object.entries(shape.properties).flatMap(([k, v]) =>
      fieldRows(v, prefix ? `${prefix}.${k}` : k, req.has(k)),
    );
  }
  if (shape.type === "array") {
    const inner = fieldRows(shape.items, `${prefix}[]`, true);
    const self = {
      path: prefix,
      type: `array (max ${shape.maxItems})`,
      description: shape.description,
      required,
    };
    return shape.items.type === "object" ? [self, ...inner] : [self];
  }
  let type: string = shape.type;
  if (shape.type === "string" && shape.enum) type = shape.enum.join(" | ");
  return [{ path: prefix, type, description: shape.description, required }];
}

const snippetJson = (value: unknown) => JSON.stringify(value, null, 2);

/**
 * Manifest-driven documentation for a connected-service action. Shown in the
 * wizard before the owner commits, and again on the secret's detail card.
 */
export function ActionDocs({
  action,
  secretName,
  compact,
}: {
  action: UseDefinition;
  secretName: string;
  compact?: boolean;
}) {
  const name = secretName || action.ui.placeholder;
  const inputExample = action.input ? exampleFor(action.input) : undefined;
  const call =
    `const result = await keychain.use(${JSON.stringify(name)}` +
    (inputExample ? `, ${snippetJson(inputExample)}` : "") +
    ");";
  const cli =
    `npx @lit-protocol/keychain@2.0.2 use ./agent-identity.json ./${name}.keychain.json ${name}` +
    (inputExample ? ` '${JSON.stringify(inputExample)}'` : "");
  const inputs = action.input ? fieldRows(action.input) : [];
  const outputs = fieldRows(action.output);
  const source = CREDENTIAL_SOURCES[action.id];
  return (
    <div className={"action-docs" + (compact ? " compact" : "")}>
      {!compact && (
        <>
          <h3>What it does</h3>
          <p>{action.description}</p>
        </>
      )}
      <p className="hint">
        The upstream provider receives the credential over TLS; this action does
        not return it to the agent. See{" "}
        <a href="/PROVIDERS.md">provider setup</a>
        for exact credential formats, including the Supabase JSON allowlist.
        Supabase secret/service_role keys bypass RLS; the allowlist is the
        policy. Do not blindly retry writes: Slack posts or Supabase inserts may
        have completed even when the result was lost. Check provider state
        first.
      </p>
      <h3>What the agent never gets</h3>
      <ul className="limits">
        <li>
          The credential itself. It is decrypted only inside the Lit enclave and
          cannot be exported, even by you. Keep your original copy elsewhere.
        </li>
        <li>
          Any network access beyond{" "}
          <strong>{action.allowedHosts.join(", ")}</strong>.
        </li>
        <li>
          More than {action.limits.maxRequests}{" "}
          {action.limits.maxRequests === 1 ? "request" : "requests"} per call,{" "}
          {Math.round(action.limits.timeoutMs / 1000)}s timeout,{" "}
          {Math.round(action.limits.maxResponseBytes / 1024)} KB of response.
        </li>
        <li>
          A different action. This choice is permanent; to do something else
          with the same credential, add it again as a different connected
          service.
        </li>
      </ul>
      <h3>How your agent calls it</h3>
      <pre className="terminal">
        <code>{call}</code>
      </pre>
      <p className="hint">
        From the CLI: <code>{cli}</code>
        <br />
        In the MCP server it is the <code>{action.id}</code> tool.
      </p>
      {inputs.length > 0 && (
        <>
          <h3>Input</h3>
          <table className="shape-table">
            <tbody>
              {inputs.map((r) => (
                <tr key={r.path}>
                  <td>
                    <code>{r.path}</code>
                    {!r.required && <small> optional</small>}
                  </td>
                  <td className="muted">{r.type}</td>
                  <td>{r.description}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      {inputs.length === 0 && <p className="hint">Takes no input.</p>}
      <h3>Result</h3>
      <table className="shape-table">
        <tbody>
          {outputs.map((r) => (
            <tr key={r.path}>
              <td>
                <code>{r.path}</code>
              </td>
              <td className="muted">{r.type}</td>
              <td>{r.description}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {!compact && (
        <>
          <h3>Credential</h3>
          <p className="hint">
            {source ? (
              <>
                Create one at{" "}
                <a href={source.url} target="_blank" rel="noreferrer">
                  {source.label}
                </a>
                . Give it the narrowest scope that still lets this action
                work.{" "}
              </>
            ) : null}
            Accepted format: <code>{action.credentialPattern}</code>
          </p>
          <p className="hint">
            Reviewed action <code>{action.id}</code> by {action.author},{" "}
            {action.license}. Pinned to a specific library commit; the same
            bytes run for the life of this secret.
          </p>
        </>
      )}
    </div>
  );
}

type Step =
  { kind: "choose" } | { kind: "catalog" } | { kind: "form"; release: string };

export function AddSecret({
  busy,
  onCreate,
  onCancel,
}: {
  busy: boolean;
  onCreate: (name: string, value: string, release: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [step, setStep] = useState<Step>({ kind: "choose" });
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [problem, setProblem] = useState("");
  const catalog = useMemo(
    () =>
      availableActions("verified").filter(
        (a): a is UseDefinition => a.kind === "use",
      ),
    [],
  );
  const cancel = () => {
    setValue("");
    onCancel();
  };
  const back = () =>
    setStep(
      step.kind === "form" && step.release !== "export"
        ? { kind: "catalog" }
        : { kind: "choose" },
    );

  if (step.kind === "choose")
    return (
      <div className="add-flow">
        <h2>Add to your keychain</h2>
        <p className="hint">
          Two ways an agent can use a credential. The choice is permanent.
        </p>
        <div className="choice-grid">
          <button
            type="button"
            className="choice-card"
            onClick={() => setStep({ kind: "form", release: "export" })}
          >
            <span className="choice-icon" aria-hidden="true">
              ⌘
            </span>
            <strong>Store a secret</strong>
            <p>
              Approved agents receive an encrypted copy and decrypt it on their
              own machine. Works for anything: API keys, tokens, passwords.
            </p>
            <small>The agent sees the value.</small>
          </button>
          <button
            type="button"
            className="choice-card"
            onClick={() => setStep({ kind: "catalog" })}
          >
            <span className="choice-icon" aria-hidden="true">
              ⚡
            </span>
            <strong>Connect a service</strong>
            <p>
              Approved agents run one reviewed action with the credential inside
              the Lit enclave. Stripe, OpenAI, GitHub, Slack.
            </p>
            <small>The agent never sees the key.</small>
          </button>
        </div>
        <button type="button" className="ghost" onClick={cancel}>
          Cancel
        </button>
      </div>
    );

  if (step.kind === "catalog")
    return (
      <div className="add-flow">
        <button type="button" className="ghost back" onClick={back}>
          ← Back
        </button>
        <h2>Connect a service</h2>
        <p className="hint">
          Each action is a fixed, reviewed program. It reaches only the hosts
          listed, returns only the fields documented, and can never hand the
          credential to the agent.
        </p>
        <div className="catalog-grid">
          {catalog.map((action) => (
            <button
              type="button"
              className="catalog-card"
              key={action.id}
              onClick={() => {
                setName("");
                setStep({ kind: "form", release: action.id });
              }}
            >
              <span className="eyebrow">
                {CATEGORY_LABELS[action.category] ?? action.category}
              </span>
              <strong>{action.name}</strong>
              <p>{action.description}</p>
              <small>Reaches {action.allowedHosts.join(", ")}</small>
            </button>
          ))}
        </div>
        <p className="hint">
          Need another service? Actions are added by pull request to the public{" "}
          <a
            href="https://github.com/LIT-Protocol/agent-keychain-library"
            target="_blank"
            rel="noreferrer"
          >
            agent-keychain-library
          </a>{" "}
          repository.
        </p>
      </div>
    );

  const action = catalog.find((a) => a.id === step.release);
  const isUse = !!action;
  return (
    <div className="add-flow">
      <button type="button" className="ghost back" onClick={back}>
        ← Back
      </button>
      <p className="eyebrow">
        {isUse ? "CONNECT A SERVICE" : "STORE A SECRET"}
      </p>
      <h2>{isUse ? action.name : "Add a secret"}</h2>
      {isUse ? (
        <ActionDocs action={action} secretName={name} />
      ) : (
        <p className="hint">
          The value is encrypted in your browser before upload. When you approve
          an agent, the Lit enclave re-encrypts it to that agent's key and the
          agent decrypts it locally. No agents have access until you approve
          one.
        </p>
      )}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (isUse && !new RegExp(action.credentialPattern).test(value)) {
            setProblem(
              `That does not look like a credential for ${action.name}. Check the accepted format above.`,
            );
            return;
          }
          setProblem("");
          // Success unmounts this component; on failure the values stay so the
          // owner can correct them.
          void onCreate(name, value, step.release);
        }}
      >
        <label>
          Name
          <input
            required
            pattern="[A-Z][A-Z0-9_]{0,63}"
            value={name}
            onChange={(e) => setName(e.target.value.toUpperCase())}
            placeholder={action?.ui.placeholder ?? "API_KEY"}
            autoComplete="off"
          />
        </label>
        <label>
          {isUse ? "Credential" : "Secret value"}
          <textarea
            required
            value={value}
            onChange={(e) => setValue(e.target.value)}
            autoComplete="off"
            spellCheck={false}
            placeholder="Encrypted before upload"
          />
        </label>
        {problem && <p className="hint problem">{problem}</p>}
        <button disabled={busy}>
          {isUse ? "Encrypt & connect" : "Encrypt & save"}
        </button>
        <button type="button" className="ghost" onClick={cancel}>
          Cancel
        </button>
      </form>
    </div>
  );
}
