import { describe, expect, it } from "vitest";

import fixture from "@/plan/api/fixtures/trace.json";
import { traceViewSchema, type TraceView } from "@/plan/api/schema-trace";

import {
  changeTarget,
  coverageOfHeading,
  implementedHeadings,
  pressureRecordText,
  sectionCoverage,
  settlePoints,
  settleWords,
} from "./doc-model";
import { coverageHref } from "./model";

const S01 = "doc/product/demo/section_01.md";
const S02 = "doc/product/demo/section_02.md";
const FIRST = "doc/plans/demo/PLAN-first.md";

function trace(): TraceView {
  return traceViewSchema.parse(structuredClone(fixture));
}

const doc = (view: TraceView, path: string) => view.documents.find((item) => item.path === path)!;

describe("settleWords", () => {
  it("says the state, and the verb for a draft", () => {
    expect(settleWords("draft", "section")).toEqual({ chip: "Draft", button: "Mark settled" });
    expect(settleWords("settled", "plan").button).toBe("Settled");
    expect(settleWords("revised", "section").button).toBe("Changed since you settled it");
    expect(settleWords("stub_changed", "plan").chip).toBe("Changed since you settled it");
  });

  it("names a plan's moved spec", () => {
    expect(settleWords("upstream_changed", "plan")).toEqual({
      chip: "Spec changed since you settled it",
      button: "Spec changed since you settled this plan",
    });
    expect(settleWords("upstream_changed", "section").chip).toBe(
      "Upstream changed since you settled it",
    );
  });
});

describe("coverageOfHeading", () => {
  it("names the plan and stage covering a heading, with its open findings", () => {
    expect(coverageOfHeading(trace(), S01, "two")).toEqual({
      title: "Two",
      plans: [{ slug: "first", to: "/f/demo/engineering/first", stages: ["build"] }],
      open: 1,
    });
  });

  it("lets a plan cover win over the metaplan entry of the same plan", () => {
    const covered = coverageOfHeading(trace(), S01, "one");
    expect(covered?.plans).toEqual([
      { slug: "first", to: "/f/demo/engineering/first", stages: [] },
    ]);
    expect(covered?.open).toBe(0);
  });

  it("keeps an entry alone as a plan not yet written, linked to its metaplan entry", () => {
    const covered = coverageOfHeading(trace(), S02, "three");
    expect(covered?.plans).toEqual([
      { slug: "second", to: "/f/demo/metaplan#2-second-plan-secondmd", stages: [] },
    ]);
  });

  it("returns no plans for an uncovered heading, and null for a heading the trace lacks", () => {
    const view = trace();
    view.headings.find((item) => item.path === S01 && item.anchor === "two")!.covered_by = [];
    expect(coverageOfHeading(view, S01, "two")?.plans).toEqual([]);
    expect(coverageOfHeading(view, S01, "nowhere")).toBeNull();
    expect(coverageOfHeading(view, "doc/loom/knowledge/notes.md", "one")).toBeNull();
  });

  it("counts only open findings", () => {
    const view = trace();
    view.findings[0]!.finding.status = { kind: "dismissed", detail: "no" };
    expect(coverageOfHeading(view, S01, "two")?.open).toBe(0);
  });
});

describe("sectionCoverage and coverageHref", () => {
  it("counts the headings anything covers", () => {
    const view = trace();
    expect(sectionCoverage(view, S01)).toEqual({ covered: 2, total: 2 });
    view.headings.find((item) => item.path === S01 && item.anchor === "two")!.covered_by = [];
    expect(sectionCoverage(view, S01)).toEqual({ covered: 1, total: 2 });
  });

  it("addresses a section by its file stem and a heading under it", () => {
    expect(coverageHref("demo", S01)).toBe("/f/demo/coverage#section_01");
    expect(coverageHref("demo", S01, "two")).toBe("/f/demo/coverage#section_01%3Atwo");
  });
});

describe("implementedHeadings", () => {
  it("lists the headings a plan or its stages cover, not those only its entry names", () => {
    const view = trace();
    const rows = implementedHeadings(view, FIRST);
    expect(rows.map(({ heading }) => heading.anchor)).toEqual(["one", "two"]);
    expect(rows[0]!.section?.title).toBe("Alpha");
    expect(implementedHeadings(view, "doc/plans/demo/PLAN-second.md")).toEqual([]);
  });
});

describe("changeTarget", () => {
  it("compares a revised document with itself", () => {
    const view = trace();
    expect(changeTarget(view, doc(view, S02))).toEqual({ settled: S02, compare: S02 });
  });

  it("compares a changed stub's holder against the document's settled version", () => {
    const view = trace();
    const section = doc(view, S02);
    section.settle = "stub_changed";
    expect(changeTarget(view, section)).toEqual({
      settled: S02,
      compare: "doc/product/demo/index.md",
    });
  });

  it("compares the revised section a plan with a moved spec covers, else nothing", () => {
    const view = trace();
    const plan = doc(view, FIRST);
    plan.settle = "upstream_changed";
    expect(changeTarget(view, plan)).toBeNull();
    doc(view, S01).settle = "revised";
    expect(changeTarget(view, plan)).toEqual({ settled: S01, compare: S01 });
  });

  it("offers nothing for a settled or draft document", () => {
    const view = trace();
    expect(changeTarget(view, doc(view, S01))).toBeNull();
  });
});

/** Line numbers as the server counts them: fence marks keep their place, so spans include them. */
const SECTIONS = [
  { level: 2, title: "Coverage", anchor: "coverage", start_line: 3, end_line: 8 },
  { level: 3, title: "Detail", anchor: "detail", start_line: 5, end_line: 8 },
  { level: 2, title: "Open questions", anchor: "open-questions", start_line: 9, end_line: 12 },
];
const TEXT = [
  "# Doc",
  "",
  "## Coverage",
  "Coverage reads **the metaplan** [Covers](x). (My call.)",
  "### Detail",
  "```",
  "(My call.) inside a fence is ignored",
  "```",
  "## Open questions",
  "- Should it come back?",
  "  - nested question",
  "1. Numbered one",
].join("\n");

describe("settlePoints", () => {
  const points = settlePoints({ text: TEXT, sections: SECTIONS });

  it("quotes a My call line without its marker or its markup, under its heading", () => {
    expect(points[0]).toEqual({
      kind: "call",
      quote: "Coverage reads the metaplan Covers.",
      where: { title: "Coverage", anchor: "coverage" },
      index: 0,
    });
  });

  it("skips code fences and reads the top-level items under Open questions in order", () => {
    expect(points.map((point) => [point.kind, point.quote, point.index])).toEqual([
      ["call", "Coverage reads the metaplan Covers.", 0],
      ["question", "Should it come back?", 1],
      ["question", "Numbered one", 2],
    ]);
  });

  it("counts open questions by the server's rules", () => {
    const text = [
      "## Open questions",
      "- Q1",
      "  - sub-point",
      "+ plus item",
      "1) paren item",
      "* Star item",
      "~~~",
      "- fenced",
      "~~~",
      "```js `x`",
      "- after an inline code line",
      "## Open question",
      "- singular heading",
    ].join("\n");
    const sections = [
      { level: 2, title: "Open questions", anchor: "open-questions", start_line: 1, end_line: 11 },
      { level: 2, title: "Open question", anchor: "open-question", start_line: 12, end_line: 13 },
    ];
    expect(settlePoints({ text, sections }).map((point) => point.quote)).toEqual([
      "Q1",
      "Star item",
      "after an inline code line",
    ]);
  });

  it("has nothing for a document with neither", () => {
    expect(settlePoints({ text: "# Doc\n\nPlain.", sections: [] })).toEqual([]);
  });
});

describe("settlePoints quotes whole sentences", () => {
  const quotes = (text: string, sections: Parameters<typeof settlePoints>[0]["sections"] = []) =>
    settlePoints({ text, sections }).map((point) => point.quote);

  it("quotes the sentence before a marker that a hard wrap left on the next line", () => {
    const text =
      "Earlier sentence. Coverage reads the metaplan's Covers: lines, never\nthe plan YAML. (My call.)";
    expect(quotes(text)).toEqual([
      "Coverage reads the metaplan's Covers: lines, never the plan YAML.",
    ]);
  });

  it("strips a My call that runs onto the next line and keeps the sentence before it", () => {
    const text = [
      "Code is not writable at all. Changes go through `loom run`. (My call: a planning session",
      "that edits code produces changes no stage reviewed.)",
      "",
      "Next paragraph.",
    ].join("\n");
    expect(quotes(text)).toEqual(["Changes go through loom run."]);
  });

  it("quotes the explanation of a marker that opens its list item", () => {
    const text = [
      "- (My call: a plan's open questions",
      "  matter as much as a section's.)",
      "- Plain item.",
    ].join("\n");
    expect(quotes(text)).toEqual(["a plan's open questions matter as much as a section's."]);
  });

  it("reads two markers of one paragraph as two points, each with its own sentence", () => {
    const text = "First is so. (My call.) Second is so (My call: because). Third stands.";
    expect(quotes(text)).toEqual(["First is so.", "Second is so"]);
  });

  it("puts a marker on the heading its own line sits under", () => {
    const sections = [
      { level: 2, title: "One", anchor: "one", start_line: 1, end_line: 3 },
      { level: 2, title: "Two", anchor: "two", start_line: 4, end_line: 7 },
    ];
    const text = "## One\nAbove.\n\n## Two\nA sentence that wraps\nonto a second line. (My call.)";
    const [point] = settlePoints({ text, sections });
    expect(point?.where).toEqual({ title: "Two", anchor: "two" });
    expect(point?.quote).toBe("A sentence that wraps onto a second line.");
  });
});

describe("pressureRecordText", () => {
  const text = "# Plan\n\n## Pressure tests\n\n- 2026-10-19: older\n- 2026-10-20: newer one\n";
  const sections = [
    { level: 2, title: "Pressure tests", anchor: "pressure-tests", start_line: 3, end_line: 6 },
  ];

  it("reads the record dated as asked from the Pressure tests section", () => {
    expect(pressureRecordText({ text, sections }, "2026-10-20")).toBe("newer one");
    expect(pressureRecordText({ text, sections }, "2026-10-21")).toBeNull();
  });

  it("ignores a dated line outside the section", () => {
    const loose = { text: "# P\n\n- 2026-10-20: loose\n", sections: [] };
    expect(pressureRecordText(loose, "2026-10-20")).toBeNull();
  });
});
