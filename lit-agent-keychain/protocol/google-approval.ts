import { z } from "zod";
import { V, hashSchema, ownerSchema, googleSessionSchema } from "./schema.ts";

export function parseGoogleApproval(value: unknown) {
  return z
    .strictObject({
      payload: z.strictObject({
        v: z.literal(V),
        domain: z.literal("lit-keychain/google-approval/v2"),
        vaultId: hashSchema,
        owner: ownerSchema,
        session: googleSessionSchema,
      }),
      signature: z.string().regex(/^[0-9a-f]{128}$/),
    })
    .parse(value);
}
export type GoogleApproval = ReturnType<typeof parseGoogleApproval>;
