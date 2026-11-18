import { describe, expect, it } from "vitest";

import traceFixture from "@/plan/api/fixtures/trace.json";
import { traceViewSchema, type TraceView } from "@/plan/api/schema-trace";

import { buildCoverLookup, sectionsToRead } from "./cover-lookup";

const S01 = "doc/product/demo/section_01.md";
const S02 = "doc/product/demo/section_02.md";

function fixture(): TraceView {
  return traceViewSchema.parse(structuredClone(traceFixture));
}

describe("cover lookup", () => {
  it("resolves sections by number with the trace's level-2 headings", () => {
    const lookup = buildCoverLookup(fixture(), new Map());
    const alpha = lookup(1);
    expect(alpha).toMatchObject({ path: S01, title: "Alpha", complete: true });
    expect(alpha?.headings.get("one")).toBe("One");
    expect(lookup(2)?.headings.get("three")).toBe("Three");
    expect(lookup(9)).toBeNull();
  });

  it("drops the section number a title opens with", () => {
    const trace = fixture();
    const beta = trace.documents.find((doc) => doc.path === S02);
    if (beta) beta.title = "02 — Beta";
    expect(buildCoverLookup(trace, new Map())(2)?.title).toBe("Beta");
  });

  it("reads only sections whose covered anchors the trace does not list", () => {
    const trace = fixture();
    expect(sectionsToRead(trace)).toEqual([]);
    trace.entries[0]?.covers.push({ path: S02, anchor: "deeper" });
    expect(sectionsToRead(trace).map((doc) => doc.path)).toEqual([S02]);
  });

  it("adds the read outline's deeper headings and is pending until it arrives", () => {
    const trace = fixture();
    expect(buildCoverLookup(trace, new Map([[S02, null]]))(2)?.complete).toBe(false);
    const outline = new Map([
      ["three", "Three"],
      ["deeper", "A deeper heading"],
    ]);
    const read = buildCoverLookup(trace, new Map([[S02, outline]]))(2);
    expect(read?.complete).toBe(true);
    expect(read?.headings.get("deeper")).toBe("A deeper heading");
    expect(read?.headings.get("four")).toBe("Four");
    const failed = buildCoverLookup(trace, new Map([[S02, "failed" as const]]))(2);
    expect(failed?.complete).toBe(true);
    expect(failed?.headings.has("deeper")).toBe(false);
  });
});
