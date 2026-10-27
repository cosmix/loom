import { describe, expect, it } from "vitest";
import overview from "@/plan/api/fixtures/concept-overview.json";
import summary from "@/plan/api/fixtures/concept-summary.json";
import {
  conceptChangedSchema,
  conceptMessageSchemas,
  conceptPageSchema,
  emptyStateSchema,
  outOfDateSchema,
} from "./schema-concept";

describe("concept wire schemas", () => {
  it.each([overview, summary])("parses a concept fixture and rejects extra fields", (fixture) => {
    expect(conceptPageSchema.safeParse(fixture).success).toBe(true);
    expect(conceptPageSchema.safeParse({ ...fixture, extra: true }).success).toBe(false);
    expect(conceptPageSchema.safeParse({ ...fixture, file_hash: null }).success).toBe(true);
  });

  it.each(["source_changed", "rebuilding", "awaiting_approval", "cannot_rebuild", "failed"])(
    "parses out-of-date reason %s",
    (reason) => {
      expect(outOfDateSchema.safeParse({ reason, detail: null }).success).toBe(true);
    },
  );

  it.each([
    "no_vision",
    "sections_no_vision",
    "no_metaplan",
    "plans_no_metaplan",
    "never_built",
    "building",
  ])("parses empty kind %s", (kind) => {
    expect(emptyStateSchema.safeParse({ kind, count: null, offer: null, docs: [] }).success).toBe(
      true,
    );
  });

  it("validates tagged variants, integer bounds, and flattened change messages", () => {
    expect(conceptPageSchema.safeParse({ ...overview, colour: 256 }).success).toBe(false);
    expect(
      conceptPageSchema.safeParse({ ...overview, blocks: [{ kind: "unknown" }] }).success,
    ).toBe(false);
    expect(
      emptyStateSchema.safeParse({ kind: "building", count: -1, offer: null, docs: [] }).success,
    ).toBe(false);
    const changed = { pages: ["overview"], out_of_date: [], sources: ["doc/product/vision.md"] };
    expect(conceptChangedSchema.safeParse(changed).success).toBe(true);
    expect(
      conceptMessageSchemas[0].safeParse({ type: "concept_changed", ...changed }).success,
    ).toBe(true);
    expect(conceptMessageSchemas[0].safeParse({ type: "unknown", ...changed }).success).toBe(false);
  });
});
