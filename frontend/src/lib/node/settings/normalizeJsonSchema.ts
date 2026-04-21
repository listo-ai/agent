function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function inferTypeFromValue(value: unknown): string | undefined {
  if (value === null) return "null";
  if (typeof value === "string") return "string";
  if (typeof value === "boolean") return "boolean";
  if (typeof value === "number") return Number.isInteger(value) ? "integer" : "number";
  if (Array.isArray(value)) return "array";
  if (isRecord(value)) return "object";
  return undefined;
}

function inferSchemaType(
  schema: Record<string, unknown>,
): string | string[] | undefined {
  if ("properties" in schema) return "object";
  if ("items" in schema) return "array";
  if (!("default" in schema)) return undefined;
  const fallback = inferTypeFromValue(schema["default"]);
  return fallback === "null" ? ["null", "string"] : fallback;
}

function normalizeSchemaArray(value: unknown): unknown[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.map((entry) =>
    isRecord(entry) ? normalizeJsonSchema(entry) : entry,
  );
}

export function normalizeJsonSchema(
  schema: Record<string, unknown>,
): Record<string, unknown> {
  const n: Record<string, unknown> = { ...schema };

  if (n["type"] === undefined) {
    const inferred = inferSchemaType(n);
    if (inferred !== undefined) n["type"] = inferred;
  }

  const properties = n["properties"];
  if (isRecord(properties)) {
    n["properties"] = Object.fromEntries(
      Object.entries(properties).map(([k, v]) => [
        k,
        isRecord(v) ? normalizeJsonSchema(v) : v,
      ]),
    );
  }

  const items = n["items"];
  if (isRecord(items)) n["items"] = normalizeJsonSchema(items);

  const oneOf = normalizeSchemaArray(n["oneOf"]);
  if (oneOf) n["oneOf"] = oneOf;

  const anyOf = normalizeSchemaArray(n["anyOf"]);
  if (anyOf) n["anyOf"] = anyOf;

  const allOf = normalizeSchemaArray(n["allOf"]);
  if (allOf) n["allOf"] = allOf;

  return n;
}
