// Runtime half of the catalog format: the shape subset, its zod conversion and the
// identifier grammars. This module is bundled into every action template, so it
// must stay small and change rarely; anything only the build needs lives in
// schema.ts, which imports from here.
import { z } from "zod";

export const CATALOG_FORMAT = 1 as const;
export const MAX_INPUT_BYTES = 8 * 1024;
export const MAX_OUTPUT_BYTES = 16 * 1024;
const MAX_STRING = 16 * 1024;

export const releaseIdSchema = z.string().regex(/^[a-z][a-z0-9_]{1,63}$/);
export const operationSchema = z
  .string()
  .max(64)
  .regex(/^[a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)*$/);
const fieldName = z.string().regex(/^[a-z][A-Za-z0-9]{0,63}$/);
const shortText = z.string().min(1).max(200);
const longText = z.string().min(1).max(400);

export type Shape =
  | {
      type: "object";
      description?: string;
      properties: Record<string, Shape>;
      required?: string[];
    }
  | {
      type: "string";
      description?: string;
      minLength?: number;
      maxLength: number;
      pattern?: string;
      enum?: string[];
    }
  | {
      type: "integer";
      description?: string;
      minimum?: number;
      maximum?: number;
    }
  | { type: "number"; description?: string }
  | { type: "boolean"; description?: string }
  | {
      type: "array";
      description?: string;
      items: Shape;
      maxItems: number;
    };
const safeInt = z
  .number()
  .int()
  .min(Number.MIN_SAFE_INTEGER)
  .max(Number.MAX_SAFE_INTEGER);
export const shapeSchema: z.ZodType<Shape> = z.lazy(() =>
  z.discriminatedUnion("type", [
    z
      .strictObject({
        type: z.literal("object"),
        description: shortText.optional(),
        properties: z.record(fieldName, shapeSchema),
        required: z.array(fieldName).max(32).optional(),
      })
      .refine(
        (s) =>
          Object.keys(s.properties).length <= 32 &&
          (s.required ?? []).every((k) => k in s.properties),
        "required must name declared properties (max 32)",
      ),
    z
      .strictObject({
        type: z.literal("string"),
        description: shortText.optional(),
        minLength: z.number().int().min(0).max(MAX_STRING).optional(),
        maxLength: z.number().int().min(1).max(MAX_STRING),
        pattern: z
          .string()
          .max(512)
          .refine((p) => {
            try {
              new RegExp(p, "u");
              return true;
            } catch {
              return false;
            }
          }, "invalid pattern")
          .optional(),
        enum: z.array(z.string().max(128)).min(1).max(64).optional(),
      })
      .refine(
        (s) => (s.minLength ?? 0) <= s.maxLength,
        "minLength exceeds maxLength",
      ),
    z
      .strictObject({
        type: z.literal("integer"),
        description: shortText.optional(),
        minimum: safeInt.optional(),
        maximum: safeInt.optional(),
      })
      .refine(
        (s) => (s.minimum ?? -Infinity) <= (s.maximum ?? Infinity),
        "minimum exceeds maximum",
      ),
    z.strictObject({
      type: z.literal("number"),
      description: shortText.optional(),
    }),
    z.strictObject({
      type: z.literal("boolean"),
      description: shortText.optional(),
    }),
    z.strictObject({
      type: z.literal("array"),
      description: shortText.optional(),
      items: shapeSchema,
      maxItems: z.number().int().min(0).max(1000),
    }),
  ]),
);

/** Converts a Shape to a strict zod validator. Unknown fields are rejected. */
export function shapeToZod(shape: Shape): z.ZodType {
  switch (shape.type) {
    case "object": {
      const required = new Set(shape.required ?? []);
      const fields: Record<string, z.ZodType> = {};
      for (const [key, child] of Object.entries(shape.properties)) {
        const v = shapeToZod(child);
        fields[key] = required.has(key) ? v : v.optional();
      }
      return z.strictObject(fields);
    }
    case "string": {
      let s = z.string().max(shape.maxLength);
      if (shape.minLength !== undefined) s = s.min(shape.minLength);
      if (shape.pattern !== undefined)
        s = s.regex(new RegExp(shape.pattern, "u"));
      if (shape.enum) {
        const allowed = new Set(shape.enum);
        return s.refine((v) => allowed.has(v), "not an allowed value");
      }
      return s;
    }
    case "integer": {
      let n = safeInt;
      if (shape.minimum !== undefined) n = n.min(shape.minimum);
      if (shape.maximum !== undefined) n = n.max(shape.maximum);
      return n;
    }
    case "number":
      return z.number();
    case "boolean":
      return z.boolean();
    case "array":
      return z.array(shapeToZod(shape.items)).max(shape.maxItems);
  }
}
/** Shapes are already a JSON Schema subset; this adds the closed-object marker MCP clients expect. */
export function shapeToJsonSchema(shape: Shape): Record<string, unknown> {
  switch (shape.type) {
    case "object":
      return {
        ...shape,
        properties: Object.fromEntries(
          Object.entries(shape.properties).map(([k, v]) => [
            k,
            shapeToJsonSchema(v),
          ]),
        ),
        additionalProperties: false,
      };
    case "array":
      return { ...shape, items: shapeToJsonSchema(shape.items) };
    default:
      return { ...shape };
  }
}
/** Input shapes travel inside the agent's signed request, which is canonical JSON: no floats. */
export function inputShapeIsCanonical(shape: Shape): boolean {
  switch (shape.type) {
    case "number":
      return false;
    case "object":
      return Object.values(shape.properties).every(inputShapeIsCanonical);
    case "array":
      return inputShapeIsCanonical(shape.items);
    default:
      return true;
  }
}
