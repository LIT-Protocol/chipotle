// Minimal Model Context Protocol server over stdio for the Lit Agent Keychain.
//
// It is deliberately local and dependency-free: the agent identity private key
// and the decrypted secret values never leave this process, so a hosted MCP
// endpoint would break the Keychain trust boundary (the operator must never see
// plaintext or agent private keys). One-line install for MCP clients:
//
//   claude mcp add lit-keychain -- npx -y @lit-protocol/keychain mcp ./agent-identity.json ./API_KEY.keychain.json
//
// Only JSON-RPC frames are written to stdout; diagnostics go to stderr.
import { createInterface } from "node:readline";
import {
  Keychain,
  ACTIONS,
  shapeToJsonSchema,
  assertAgentConfig,
  assertAgentIdentity,
} from "./dist/index.js";
import { peerCertificateSha256 } from "./tls.mjs";

export const PROTOCOL_VERSIONS = ["2025-06-18", "2025-03-26", "2024-11-05"];
const SERVER_INFO = { name: "lit-agent-keychain", version: "2.0.0" };
const MAX_FRAME_BYTES = 64 * 1024;

const nameProperty = {
  name: { type: "string", description: "Secret name from list_secrets" },
};
/** One MCP tool per "use inside Lit" catalog action, so a model sees each action's input schema directly. */
const USE_ACTIONS = Object.values(ACTIONS).filter(
  (action) => action.kind === "use" && !action.deprecated,
);
const TOOLS = [
  {
    name: "list_secrets",
    description:
      "List the secret names this agent may request, with the single operation the owner's chosen action permits (get for export releases, otherwise the catalog action's operation). Returns no secret values.",
    inputSchema: {
      type: "object",
      properties: {},
      additionalProperties: false,
    },
  },
  {
    name: "get_secret",
    description:
      "Decrypt one owner-approved credential (export release only) and return its value. Use the value directly; do not repeat or log it.",
    inputSchema: {
      type: "object",
      properties: nameProperty,
      required: ["name"],
      additionalProperties: false,
    },
  },
  ...USE_ACTIONS.map((action) => ({
    name: action.id,
    description: `${action.description} Runs inside Lit for a secret created with the "${action.name}" action; the credential is never returned. Result fields: ${Object.keys(action.output.properties ?? {}).join(", ")}.`,
    inputSchema: {
      type: "object",
      properties: {
        ...nameProperty,
        ...(action.input ? { input: shapeToJsonSchema(action.input) } : {}),
      },
      required: [
        "name",
        ...(action.input && (action.input.required ?? []).length
          ? ["input"]
          : []),
      ],
      additionalProperties: false,
    },
  })),
  {
    name: "list_actions",
    description:
      "Describe every catalog action this client knows: id, operation, whether the agent receives the value (export) or the value is only used inside Lit, and the input shape. Contains nothing sensitive.",
    inputSchema: {
      type: "object",
      properties: {},
      additionalProperties: false,
    },
  },
  {
    name: "agent_public_key",
    description:
      "Return this agent's Ed25519 public key. Give it to the secret owner so they can approve this agent in Keychain. Contains nothing sensitive.",
    inputSchema: {
      type: "object",
      properties: {},
      additionalProperties: false,
    },
  },
];

const text = (value) => ({
  content: [
    {
      type: "text",
      text: typeof value === "string" ? value : JSON.stringify(value),
    },
  ],
});
const failure = (message) => ({ ...text(message), isError: true });
const rpcError = (id, code, message) => ({
  jsonrpc: "2.0",
  id,
  error: { code, message },
});

/** Reads and validates the identity and one or more agent config files. */
export async function loadKeychain(
  identityFile,
  configFiles,
  { readFile, usageApiKey, attestation, tlsCertificateSha256 } = {},
) {
  if (!identityFile || configFiles.length === 0)
    throw new Error(
      "Usage: keychain mcp <identity-file> <config-file> [more-config-files]",
    );
  const parse = async (file) => {
    try {
      return JSON.parse(await readFile(file, "utf8"));
    } catch (error) {
      throw new Error(`Cannot read ${file}: ${error.message}`);
    }
  };
  const identity = await parse(identityFile);
  assertAgentIdentity(identity);
  const merged = {
    v: 2,
    litApiUrl: undefined,
    usageApiKey: undefined,
    secrets: {},
  };
  for (const file of configFiles) {
    const config = await parse(file);
    assertAgentConfig(config);
    if (merged.litApiUrl === undefined) merged.litApiUrl = config.litApiUrl;
    if (merged.litApiUrl !== config.litApiUrl)
      throw new Error(
        `${file} uses a different litApiUrl than the first config`,
      );
    if (config.usageApiKey !== undefined) {
      if (
        merged.usageApiKey !== undefined &&
        merged.usageApiKey !== config.usageApiKey
      )
        throw new Error(
          `${file} carries a different usage key than an earlier config; agent configs must come from one vault`,
        );
      merged.usageApiKey = config.usageApiKey;
    }
    for (const [name, locator] of Object.entries(config.secrets)) {
      if (
        merged.secrets[name] &&
        merged.secrets[name].actionCid !== locator.actionCid
      )
        throw new Error(
          `Secret name ${name} appears in two configs with different actions`,
        );
      merged.secrets[name] = locator;
    }
  }
  return new Keychain(identity.privateKey, merged, {
    usageApiKey,
    attestation,
    tlsCertificateSha256,
  });
}

export async function callTool(keychain, name, args = {}) {
  const secretName = () => {
    if (typeof args?.name !== "string" || args.name.length === 0)
      throw new Error("name is required");
    if (!keychain.config.secrets[args.name])
      throw new Error(
        `Unknown secret "${args.name}". Available: ${
          keychain
            .list()
            .map((s) => s.name)
            .join(", ") || "(none)"
        }`,
      );
    return args.name;
  };
  switch (name) {
    case "list_secrets":
      return text({ secrets: keychain.list() });
    case "agent_public_key":
      return text({ publicKey: keychain.publicKey });
    case "get_secret":
      return text(await keychain.get(secretName()));
    case "list_actions":
      return text({
        actions: Object.values(ACTIONS)
          .filter((action) => !action.deprecated)
          .map((action) => ({
            id: action.id,
            name: action.name,
            kind: action.kind,
            operation: action.operation,
            description: action.description,
            ...(action.kind === "use" ? { input: action.input } : {}),
          })),
      });
    default: {
      const action = USE_ACTIONS.find((candidate) => candidate.id === name);
      if (!action) return null;
      const secret = secretName();
      const release = keychain.config.secrets[secret].manifest.release;
      if (release !== action.id)
        throw new Error(
          `Secret "${secret}" was created with the ${release} action, not ${action.id}`,
        );
      if (
        args.input !== undefined &&
        (typeof args.input !== "object" ||
          args.input === null ||
          Array.isArray(args.input))
      )
        throw new Error("input must be an object");
      return text(await keychain.use(secret, args.input));
    }
  }
}

export async function handleMessage(keychain, message) {
  if (
    !message ||
    typeof message !== "object" ||
    Array.isArray(message) ||
    message.jsonrpc !== "2.0"
  )
    return rpcError(null, -32600, "Invalid Request");
  const { id, method, params } = message;
  const isRequest = id !== undefined && id !== null;
  if (typeof method !== "string")
    return isRequest ? rpcError(id, -32600, "Invalid Request") : undefined;
  if (!isRequest) return undefined; // notifications need no reply
  switch (method) {
    case "initialize": {
      const requested = params?.protocolVersion;
      const protocolVersion = PROTOCOL_VERSIONS.includes(requested)
        ? requested
        : PROTOCOL_VERSIONS[0];
      return {
        jsonrpc: "2.0",
        id,
        result: {
          protocolVersion,
          capabilities: { tools: {} },
          serverInfo: SERVER_INFO,
          instructions:
            "Secrets are decrypted locally with this agent's identity. Call list_secrets first; never echo values returned by get_secret.",
        },
      };
    }
    case "ping":
      return { jsonrpc: "2.0", id, result: {} };
    case "tools/list":
      return { jsonrpc: "2.0", id, result: { tools: TOOLS } };
    case "tools/call": {
      const name = params?.name;
      if (typeof name !== "string")
        return rpcError(id, -32602, "tools/call requires a tool name");
      try {
        const result = await callTool(keychain, name, params?.arguments ?? {});
        if (result === null)
          return rpcError(id, -32602, `Unknown tool: ${name}`);
        return { jsonrpc: "2.0", id, result };
      } catch (error) {
        // Tool failures are reported in-band so the model can recover.
        return { jsonrpc: "2.0", id, result: failure(error.message) };
      }
    }
    default:
      return rpcError(id, -32601, `Method not found: ${method}`);
  }
}

export async function serve(keychain, { input, output }) {
  const write = (frame) =>
    new Promise((resolve) =>
      output.write(JSON.stringify(frame) + "\n", resolve),
    );
  const lines = createInterface({ input, crlfDelay: Infinity });
  let pending = Promise.resolve();
  for await (const line of lines) {
    if (line.trim() === "") continue;
    if (Buffer.byteLength(line) > MAX_FRAME_BYTES) {
      await write(rpcError(null, -32600, "Frame too large"));
      continue;
    }
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      await write(rpcError(null, -32700, "Parse error"));
      continue;
    }
    // Preserve request order; a slow Lit execution must not reorder replies.
    pending = pending.then(async () => {
      const reply = await handleMessage(keychain, message);
      if (reply) await write(reply);
    });
    await pending;
  }
}

export async function main(argv, { readFile, stdin, stdout, stderr, env }) {
  const [identityFile, ...configFiles] = argv;
  const skipAttestation = env.KEYCHAIN_SKIP_ATTESTATION === "1";
  let keychain = await loadKeychain(identityFile, configFiles, {
    readFile,
    usageApiKey: env.CHIPOTLE_USAGE_API_KEY,
    attestation: skipAttestation ? false : undefined,
  });
  if (!skipAttestation && keychain.lit.attestationPolicy) {
    // Bind the endpoint's live TLS certificate into the attestation check, then
    // attest eagerly so a misconfigured or impostor endpoint fails at startup.
    const tlsCertificateSha256 = await peerCertificateSha256(
      keychain.config.litApiUrl,
    );
    keychain.destroy();
    keychain = await loadKeychain(identityFile, configFiles, {
      readFile,
      usageApiKey: env.CHIPOTLE_USAGE_API_KEY,
      tlsCertificateSha256,
    });
    const report = await keychain.attest();
    stderr.write(
      `Lit Agent Keychain MCP: attested ${report.origin} (${report.checks.join(", ")})\n`,
    );
  }
  stderr.write(
    `Lit Agent Keychain MCP: ${keychain.list().length} secret(s), agent ${keychain.publicKey}\n`,
  );
  try {
    await serve(keychain, { input: stdin, output: stdout });
  } finally {
    keychain.destroy();
  }
}
