import { describe, it, expect } from "vitest";
import { MsgSchema } from "../../src/schemas/msg.js";
import { GraphEventSchema } from "../../src/schemas/events.js";
import { CapabilityManifestSchema } from "../../src/schemas/capability.js";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// Root of the contracts fixture directory (relative to this file).
const FIXTURES = join(__dirname, "../../../contracts/fixtures");

function loadFixture(relPath: string): unknown {
  return JSON.parse(readFileSync(join(FIXTURES, relPath), "utf8"));
}

describe("contracts / msg fixtures", () => {
  it("parses value-changed.json", () => {
    const raw = loadFixture("msg/value-changed.json");
    expect(() => MsgSchema.parse(raw)).not.toThrow();
  });
});

describe("contracts / event fixtures", () => {
  it("parses slot-changed.json", () => {
    const raw = loadFixture("events/slot-changed.json");
    expect(() => GraphEventSchema.parse(raw)).not.toThrow();
  });
});

describe("contracts / capability-manifest fixture", () => {
  it("parses capability-manifest.json", () => {
    const raw = loadFixture("capability-manifest.json");
    expect(() => CapabilityManifestSchema.parse(raw)).not.toThrow();
  });
});
