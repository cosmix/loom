import { describe, expect, it } from "vitest";
import overview from "@/plan/api/fixtures/concept-overview.json";
import summary from "@/plan/api/fixtures/concept-summary.json";
import {
  conceptChangedSchema,
  conceptMessageSchemas,
  conceptPageSchema,
  docLinkSchema,
  emptyStateSchema,
  isConceptPageKey,
  outOfDateSchema,
  planLinkSchema,
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

describe("strings the page code uses as keys", () => {
  const docLink = { id: "a", title: "A", path: "doc/product/a.md", page: "product/a" };

  it.each(["written", "running", "done", "retired"])("parses plan state %s", (plan_state) => {
    expect(conceptPageSchema.safeParse({ ...summary, plan_state }).success).toBe(true);
  });

  it.each(["constructor", "__proto__", "in_progress", ""])(
    "rejects plan state %j",
    (plan_state) => {
      expect(conceptPageSchema.safeParse({ ...summary, plan_state }).success).toBe(false);
    },
  );

  it.each(["start-vision", "start-metaplan", null])("parses empty offer %j", (offer) => {
    expect(
      emptyStateSchema.safeParse({ kind: "no_vision", count: null, offer, docs: [] }).success,
    ).toBe(true);
  });

  it.each(["constructor", "__proto__", "start-run", ""])("rejects empty offer %j", (offer) => {
    expect(
      emptyStateSchema.safeParse({ kind: "no_vision", count: null, offer, docs: [] }).success,
    ).toBe(false);
  });

  it.each(["product", "engineering", "product/section_01", "engineering/feat-x.v2"])(
    "accepts the page key %s",
    (key) => {
      expect(isConceptPageKey(key)).toBe(true);
      expect(docLinkSchema.safeParse({ ...docLink, page: key }).success).toBe(true);
    },
  );

  it.each([
    "",
    "plans/x",
    "product/",
    "product/.",
    "product/..",
    "product/../../server",
    "product/a/b",
    "product/a b",
    "product/a%2Fb",
    `product/${"a".repeat(129)}`,
  ])("rejects the page key %j", (key) => {
    expect(isConceptPageKey(key)).toBe(false);
    expect(docLinkSchema.safeParse({ ...docLink, page: key }).success).toBe(false);
  });

  it("takes a plan slug by the page-name rule", () => {
    const link = { title: "A plan", state: "running" };
    expect(planLinkSchema.safeParse({ ...link, slug: "feat-planner-05" }).success).toBe(true);
    for (const slug of ["", ".", "..", "a/b", "a b", "../x"]) {
      expect(planLinkSchema.safeParse({ ...link, slug }).success).toBe(false);
    }
  });

  it.each(["written", "running", "done", "retired"])("takes the plan state %s", (state) => {
    expect(conceptPageSchema.safeParse({ ...summary, plan_state: state }).success).toBe(true);
  });

  it("rejects an unknown plan state and takes none", () => {
    expect(conceptPageSchema.safeParse({ ...summary, plan_state: "in_progress" }).success).toBe(
      false,
    );
    expect(conceptPageSchema.safeParse({ ...summary, plan_state: null }).success).toBe(true);
  });
});
