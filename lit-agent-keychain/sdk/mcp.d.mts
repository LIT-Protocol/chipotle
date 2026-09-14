import type { Keychain, AgentConfig, AttestationPolicy } from "./dist/index.js";
/** Structural view of a Keychain so src and dist builds interoperate. */
export type KeychainLike = {
  readonly config: AgentConfig;
  readonly publicKey: string;
  list(): { name: string; release: string; operation: string }[];
  get(name: string): Promise<string>;
  stripeBalance(name: string): Promise<unknown>;
  destroy(): void;
};
export const PROTOCOL_VERSIONS: string[];
export type JsonRpcMessage = {
  jsonrpc: "2.0";
  id?: number | string | null;
  method?: string;
  params?: unknown;
};
export type ToolResult = {
  content: { type: "text"; text: string }[];
  isError?: boolean;
};
export function loadKeychain(
  identityFile: string,
  configFiles: string[],
  options?: {
    readFile?: (file: string, encoding: "utf8") => Promise<string>;
    usageApiKey?: string;
    attestation?: AttestationPolicy | false;
    tlsCertificateSha256?: string;
  },
): Promise<Keychain>;
export function callTool(
  keychain: KeychainLike,
  name: string,
  args?: unknown,
): Promise<ToolResult | null>;
export function handleMessage(
  keychain: KeychainLike,
  message: unknown,
): Promise<Record<string, unknown> | undefined>;
export function serve(
  keychain: KeychainLike,
  io: { input: NodeJS.ReadableStream; output: NodeJS.WritableStream },
): Promise<void>;
export function main(
  argv: string[],
  io: {
    readFile: (file: string, encoding: "utf8") => Promise<string>;
    stdin: NodeJS.ReadableStream;
    stdout: NodeJS.WritableStream;
    stderr: NodeJS.WritableStream;
    env: Record<string, string | undefined>;
  },
): Promise<void>;
