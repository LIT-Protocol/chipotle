import { importBytes } from "ipfs-unixfs-importer";
import { fixedSize } from "ipfs-unixfs-importer/chunker";
import { balanced } from "ipfs-unixfs-importer/layout";
import templates from "../generated/templates.ts";
import { canonical } from "./crypto.ts";
import {
  authoritySchema,
  manifestSchema,
  type Authority,
  type Manifest,
} from "./schema.ts";
export function actionSource(manifest: Authority | Manifest): string {
  const isAuthority = "owner" in manifest;
  const validated = isAuthority
    ? authoritySchema.parse(manifest)
    : manifestSchema.parse(manifest);
  const base = isAuthority
    ? templates.authority
    : templates[(manifest as Manifest).release];
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
export const actionCid = async (manifest: Authority | Manifest) =>
  cidForCode(actionSource(manifest));
