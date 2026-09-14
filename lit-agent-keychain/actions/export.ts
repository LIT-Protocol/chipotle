import { execute } from "./secret-common.ts";
import type { Manifest } from "../protocol/schema.ts";
export const run = (manifest: Manifest, params: unknown) =>
  execute(manifest, params, "export", "get", async (value) => value);
