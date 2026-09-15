// Build-time catalog definition format. Every action ships an action.json
// validated by `definitionSchema`; the harness enforces the declared constraints
// (credential shape, reachable hosts, input/output shapes, limits) at runtime using
// the shape helpers in shape.ts, so a catalog entry is bounded by its manifest,
// not only by review. Nothing here is bundled into action templates.
import { z } from "zod";
import {
  CATALOG_FORMAT,
  releaseIdSchema,
  operationSchema,
  shapeSchema,
  inputShapeIsCanonical,
} from "./shape.ts";
export * from "./shape.ts";

const longText = z.string().min(1).max(400);
const hostSchema = z
  .string()
  .max(253)
  .regex(/^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$/);
const useDefinition = z.strictObject({
  v: z.literal(CATALOG_FORMAT),
  id: releaseIdSchema,
  kind: z.literal("use"),
  name: z.string().min(1).max(64),
  description: longText,
  category: z.enum([
    "payments",
    "ai",
    "developer",
    "messaging",
    "data",
    "other",
  ]),
  author: z.string().min(1).max(64),
  license: z.string().min(1).max(32),
  operation: operationSchema,
  credentialPattern: z
    .string()
    .min(1)
    .max(512)
    .refine((p) => {
      try {
        new RegExp(p);
        return true;
      } catch {
        return false;
      }
    }, "invalid credential pattern"),
  allowedHosts: z.array(hostSchema).min(1).max(8),
  input: shapeSchema.nullable(),
  output: shapeSchema,
  limits: z.strictObject({
    timeoutMs: z.number().int().min(1000).max(30000),
    maxResponseBytes: z
      .number()
      .int()
      .min(1024)
      .max(1024 * 1024),
    maxRequests: z.number().int().min(1).max(4),
  }),
  ui: z.strictObject({
    label: z.string().min(1).max(64),
    hint: z.string().min(1).max(240),
    placeholder: z.string().regex(/^[A-Z][A-Z0-9_]{0,63}$/),
  }),
  tier: z.enum(["verified", "community"]),
  deprecated: z.boolean(),
});
const exportDefinition = z.strictObject({
  v: z.literal(CATALOG_FORMAT),
  id: z.literal("export"),
  kind: z.literal("export"),
  name: z.string().min(1).max(64),
  description: longText,
  category: z.literal("other"),
  author: z.string().min(1).max(64),
  license: z.string().min(1).max(32),
  operation: z.literal("get"),
  ui: useDefinition.shape.ui,
  tier: z.literal("verified"),
  deprecated: z.boolean(),
});
export const definitionSchema = z
  .discriminatedUnion("kind", [useDefinition, exportDefinition])
  .refine(
    (d) =>
      d.kind === "export" || d.input === null || inputShapeIsCanonical(d.input),
    "input shapes may not contain floating point numbers",
  )
  .refine(
    (d) => d.kind === "export" || d.operation !== "get",
    "the get operation is reserved for the export action",
  );
export type ActionDefinition = z.infer<typeof definitionSchema>;
export type UseDefinition = z.infer<typeof useDefinition>;

/** Catalog as emitted by the build: definitions keyed by release id. */
export type Catalog = Record<string, ActionDefinition>;
export const catalogSchema = z
  .record(releaseIdSchema, definitionSchema)
  .refine(
    (c) => Object.entries(c).every(([id, d]) => id === d.id),
    "catalog keys must equal definition ids",
  )
  .refine((c) => "export" in c, "the export action is required")
  .refine((c) => {
    const ops = Object.values(c).map((d) => d.operation);
    return new Set(ops).size === ops.length;
  }, "operations must be unique");
