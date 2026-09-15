// Static checks for catalog action sources, shared by the build and the tests.
// Anything that could reach the network, the Lit runtime or executable code
// outside the harness is rejected before bundling: the harness's bound client is
// the only I/O path an action gets.
export const ALLOWED_IMPORT = "../../lib.ts";
export const BANNED: [RegExp, string][] = [
  [/\bfetch\s*\(/, "direct fetch (use context.fetchJson / fetchText)"],
  [
    /\bXMLHttpRequest\b|\bWebSocket\b|\bEventSource\b|\bnavigator\b/,
    "browser network API",
  ],
  [/\beval\b|\bnew\s+Function\b|\bFunction\s*\(/, "dynamic code"],
  [/\bimport\s*\(/, "dynamic import"],
  [/\brequire\s*\(/, "CommonJS require"],
  [
    /\bLit\b|\bDeno\b|\bprocess\b|\bglobalThis\b|\bwindow\b|\bself\b/,
    "runtime global",
  ],
  [
    /\bsetTimeout\b|\bsetInterval\b|\bqueueMicrotask\b/,
    "timers (the harness owns deadlines)",
  ],
  [
    /\bcrypto\b|\bCryptoKey\b|\bSubtleCrypto\b/,
    "crypto (actions project data, they do not wrap keys)",
  ],
];
/** Returns the list of violations for one action.ts source; empty means it passes. */
export function lintActionSource(source: string): string[] {
  const problems: string[] = [];
  const imports = [
    ...source.matchAll(/^\s*import\b[^;]*?from\s*["']([^"']+)["']/gm),
  ].map((m) => m[1]);
  if (/^\s*import\s*["']/m.test(source))
    problems.push("side-effect imports are not allowed");
  for (const specifier of imports)
    if (specifier !== ALLOWED_IMPORT)
      problems.push(
        `may only import from "${ALLOWED_IMPORT}" (found ${specifier})`,
      );
  if (!/^\s*export\s+default\b/m.test(source))
    problems.push("must export default defineAction(...)");
  const stripped = source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
  for (const [pattern, why] of BANNED)
    if (pattern.test(stripped)) problems.push(`uses ${why}`);
  return problems;
}
