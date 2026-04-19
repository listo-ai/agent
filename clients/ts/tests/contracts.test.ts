import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { MsgSchema } from "../src/schemas/msg.js";
import { GraphEventSchema } from "../src/schemas/events.js";
import { CapabilityManifestSchema } from "../src/schemas/capability.js";

/**
 * Stage 3a-4 wire-shape contract — TS half.
 *
 * Round-trips every fixture under `/clients/contracts/fixtures/`
 * through its matching zod schema. On parse success we also reserialise
 * and assert structural equality with the original, which pins the
 * schema's passthrough / optional-field handling. Matches the Rust
 * round-trip in `crates/spi/tests/contract_fixtures_msg.rs` and
 * `crates/graph/tests/contract_fixtures_events.rs`.
 */

const FIXTURES = join(fileURLToPath(new URL(".", import.meta.url)), "../../contracts/fixtures");

function loadJson(relPath: string): unknown {
  return JSON.parse(readFileSync(join(FIXTURES, relPath), "utf8"));
}

function listFixtures(dir: string): string[] {
  return readdirSync(join(FIXTURES, dir))
    .filter((name) => name.endsWith(".json"))
    .sort();
}

describe("contracts / msg fixtures", () => {
  const files = listFixtures("msg");

  it.each(files)("parses + round-trips msg/%s", (file) => {
    const original = loadJson(`msg/${file}`);
    const parsed = MsgSchema.parse(original);
    // passthrough() preserves unknown fields, so structural equality holds.
    expect(parsed).toEqual(original);
  });

  it("has at least one fixture", () => {
    expect(files.length).toBeGreaterThan(0);
  });
});

describe("contracts / event fixtures", () => {
  const files = listFixtures("events");

  it.each(files)("parses + discriminates events/%s", (file) => {
    const original = loadJson(`events/${file}`) as { event: string };
    const parsed = GraphEventSchema.parse(original);
    expect(parsed.event).toBe(original.event);
  });

  // Every Rust `GraphEvent` variant must have a fixture — matching the
  // `every_variant_has_a_fixture` guard in the Rust round-trip test.
  const expectedVariants = [
    "node_created",
    "node_removed",
    "node_renamed",
    "slot_changed",
    "lifecycle_transition",
    "link_added",
    "link_removed",
    "link_broken",
  ];

  it.each(expectedVariants)("fixture exists for event %s", (variant) => {
    const found = files.some((f) => {
      const raw = loadJson(`events/${f}`) as { event?: string };
      return raw.event === variant;
    });
    expect(found).toBe(true);
  });
});

describe("contracts / capability-manifest fixture", () => {
  it("parses capability-manifest.json", () => {
    const raw = loadJson("capability-manifest.json");
    expect(() => CapabilityManifestSchema.parse(raw)).not.toThrow();
  });
});
