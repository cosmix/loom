import { describe, expect, it } from "vitest";

import traceFixture from "@/plan/api/fixtures/trace.json";
import { traceViewSchema, type TraceView } from "@/plan/api/schema-trace";

import { instrumentChild, instrumentOf } from "./spine-instruments";

const fixture = (): TraceView => traceViewSchema.parse(structuredClone(traceFixture));

describe("the Findings instrument", () => {
  it("counts the open and decided findings, and names the groups the page draws, newest run first", () => {
    const trace = fixture();
    // F-1 is open from the run of `first`; F-2 is dismissed with no run review (Reviews).
    expect(instrumentOf("findings", "demo", trace)).toEqual({
      id: "findings",
      reading: { open: 1, decided: 1, sources: [{ run: "first" }, { run: null }] },
    });

    trace.findings[1]!.finding.source = "REVIEW-DONE-PLAN-demo-02-second.md#open-findings";
    expect(instrumentOf("findings", "demo", trace).reading).toMatchObject({
      sources: [{ run: "02-second" }, { run: "first" }],
    });

    for (const { finding } of trace.findings) {
      finding.status = { kind: "dismissed", detail: "not needed" };
    }
    expect(instrumentOf("findings", "demo", trace).reading).toMatchObject({ open: 0, decided: 2 });
  });

  it("reads an empty ledger as nothing open, nothing decided and no source", () => {
    const trace = fixture();
    trace.findings = [];
    expect(instrumentOf("findings", "demo", trace).reading).toEqual({
      open: 0,
      decided: 0,
      sources: [],
    });
  });

  it("reads nothing before the trace loads", () => {
    expect(instrumentOf("findings", "demo", null)).toEqual({ id: "findings", reading: null });
  });
});

describe("the Coverage instrument", () => {
  it("counts the written sections' headings, one dot per heading grouped by section", () => {
    const trace = fixture();
    // Sections 01 and 02 carry two headings each, all covered; 03 is a stub with none.
    expect(instrumentOf("coverage", "demo", trace)).toEqual({
      id: "coverage",
      reading: {
        covered: 4,
        total: 4,
        sections: [
          { number: "01", headings: [true, true] },
          { number: "02", headings: [true, true] },
        ],
        gaps: [],
      },
    });
  });

  it("hollows the dot of a heading nothing covers and names its section as a gap", () => {
    const trace = fixture();
    trace.headings[0]!.covered_by = [];
    trace.headings[3]!.covered_by = [];
    expect(instrumentOf("coverage", "demo", trace).reading).toEqual({
      covered: 2,
      total: 4,
      sections: [
        { number: "01", headings: [false, true] },
        { number: "02", headings: [true, false] },
      ],
      gaps: ["01", "02"],
    });
  });

  it("draws no group for a written section with no heading yet", () => {
    const trace = fixture();
    const third = trace.documents.find((doc) => doc.path.endsWith("section_03.md"))!;
    third.progress = "written";
    expect(instrumentOf("coverage", "demo", trace).reading).toMatchObject({
      total: 4,
      sections: [{ number: "01" }, { number: "02" }],
    });
  });

  it("reads a zero total and no section for a trace with no written section", () => {
    const trace = fixture();
    for (const doc of trace.documents) if (doc.role === "section") doc.progress = "stub";
    expect(instrumentOf("coverage", "demo", trace).reading).toEqual({
      covered: 0,
      total: 0,
      sections: [],
      gaps: [],
    });
  });

  it("reads nothing before the trace loads", () => {
    expect(instrumentOf("coverage", "demo", null)).toEqual({ id: "coverage", reading: null });
  });
});

it("an instrument's child is a plain, linked, glyphless child named for its page", () => {
  expect(instrumentChild("coverage", "atlas", null)).toEqual({
    key: "coverage",
    to: "/f/atlas/coverage",
    name: "Coverage",
    style: "plain",
    number: null,
    title: null,
    note: null,
    pressure: null,
    glyph: null,
    instrument: { id: "coverage", reading: null },
  });
  expect(instrumentChild("findings", "atlas", null).to).toBe("/f/atlas/findings");
});
