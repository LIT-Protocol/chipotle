import { importBytes } from "ipfs-unixfs-importer";
import { fixedSize } from "ipfs-unixfs-importer/chunker";
import { balanced } from "ipfs-unixfs-importer/layout";
import { sha256 } from "@noble/hashes/sha2.js";
import templates from "../generated/templates.ts";
import archiveIndex from "../generated/archive-index.ts";
import { canonical, hex, utf8 } from "./crypto.ts";
import { textFetch } from "./http.ts";
import {
  authoritySchema,
  manifestSchema,
  type Authority,
  type Manifest,
} from "./schema.ts";
/** Which template a manifest binds: the authority action or a catalog release. */
export const templateName = (manifest: Authority | Manifest) =>
  "owner" in manifest ? "authority" : (manifest as Manifest).release;
/**
 * Full action source for a manifest: the template followed by the canonical
 * manifest constant. `template` overrides the bundled current release with the
 * exact bytes of an earlier one (see `TemplateStore`).
 */
export function actionSource(
  manifest: Authority | Manifest,
  template?: string,
): string {
  const isAuthority = "owner" in manifest;
  const validated = isAuthority
    ? authoritySchema.parse(manifest)
    : manifestSchema.parse(manifest);
  const base = template ?? templates[templateName(manifest)];
  if (typeof base !== "string")
    throw new Error(
      `Unknown action release ${(manifest as Manifest).release}; update the Keychain client`,
    );
  return (
    base +
    "\nconst KEYCHAIN_MANIFEST=" +
    canonical(validated) +
    ";\nasync function main(params){return KeychainAction.run(KEYCHAIN_MANIFEST,params)}\n"
  );
}
export async function cidForCode(code: string): Promise<string> {
  const result = await importBytes(
    new TextEncoder().encode(code),
    { put: async (cid) => cid },
    {
      cidVersion: 0,
      rawLeaves: false,
      leafType: "file",
      chunker: fixedSize({ chunkSize: 262144 }),
      layout: balanced({ maxChildrenPerNode: 174 }),
    },
  );
  return result.cid.toString();
}
export const actionCid = async (
  manifest: Authority | Manifest,
  template?: string,
) => cidForCode(actionSource(manifest, template));
export const templateHash = (code: string) => hex(sha256(utf8(code)));
/** One released template version: exact bytes and their SHA-256. */
export type Template = { hash: string; code: string };
/**
 * Every template version this client knows how to trust. Current releases are
 * bundled; earlier releases are known by hash (from the build's archive index)
 * and their bytes fetched from a Keychain registry on demand, then verified
 * against that hash. Only hashes in the bundled index are ever accepted, so the
 * registry cannot introduce a template this client was not built to trust.
 */
export class TemplateStore {
  private readonly cache = new Map<string, string>();
  constructor(readonly timeoutMs = 15000) {
    for (const code of Object.values(templates))
      this.cache.set(templateHash(code), code);
  }
  /** Known release hashes for a template, newest first. */
  hashes(name: string): string[] {
    return archiveIndex[name] ?? [];
  }
  async byHash(
    name: string,
    hash: string,
    registry: string,
  ): Promise<Template> {
    if (!this.hashes(name).includes(hash))
      throw new Error(`Unknown ${name} release; update the Keychain client`);
    let code = this.cache.get(hash);
    if (code === undefined) {
      code = await textFetch(
        `${registry}/api/templates/${hash}`,
        {},
        this.timeoutMs,
        4 * 1024 * 1024,
      );
      if (templateHash(code) !== hash)
        throw new Error(
          "Registry served a template that does not match its hash",
        );
      this.cache.set(hash, code);
    }
    return { hash, code };
  }
  /** The newest release of a manifest's template. */
  current(manifest: Authority | Manifest): Template {
    const code = templates[templateName(manifest)];
    if (typeof code !== "string")
      throw new Error(`Unknown action release ${templateName(manifest)}`);
    return { hash: templateHash(code), code };
  }
  /**
   * The release whose action for `manifest` has exactly `cid`, trying the current
   * release first and then each earlier one. Throws if no known release matches:
   * either the CID was never legitimate or this client predates the release.
   */
  async resolve(
    manifest: Authority | Manifest,
    cid: string,
  ): Promise<Template> {
    const name = templateName(manifest);
    for (const hash of this.hashes(name)) {
      let template: Template;
      try {
        template = await this.byHash(name, hash, manifest.registry);
      } catch {
        continue;
      }
      if ((await actionCid(manifest, template.code)) === cid) return template;
    }
    throw new Error("Pinned action CID does not match any known release");
  }
}
export const templateStore = new TemplateStore();
