import { describe, expect, it } from "vitest";

import traceFixture from "@/plan/api/fixtures/trace.json";
import {
  traceViewSchema,
  type Gap,
  type SettleState,
  type TraceView,
} from "@/plan/api/schema-trace";

import {
  bundleLinks,
  chipsFor,
  docOfPath,
  frontierLinks,
  impactFor,
  settleLabel,
  type EdgeChip,
} from "./model";

const VISION = "doc/product/demo/index.md";
const S01 = "doc/product/demo/section_01.md";
const S02 = "doc/product/demo/section_02.md";
const S03 = "doc/product/demo/section_03.md";
const METAPLAN = "doc/plans/demo/index.md";
const FIRST = "doc/plans/demo/PLAN-first.md";
const BRIEF = "doc/plans/demo/briefs/first/build/r1.md";

function fixture(): TraceView {
  return traceViewSchema.parse(structuredClone(traceFixture));
}

function setSettle(trace: TraceView, path: string, state: SettleState): void {
  docOfPath(trace, path)!.settle = state;
}

function gap(
  kind: Gap["kind"],
  path: string,
  anchor: string | null = null,
  count: number | null = null,
): Gap {
  return { kind, node: { path, anchor }, count };
}

function sentence(restsOn: [string, string][], citations: [string, string][] = []) {
  return {
    rests_on: restsOn.map(([path, anchor]) => ({ path, anchor })),
    citations: citations.map(([path, anchor]) => ({ kind: "source" as const, path, anchor })),
  };
}

function labels(chips: EdgeChip[]): string[] {
  return chips.map((chip) => `${chip.kind}:${chip.label}`);
}

describe("settleLabel", () => {
  it.each([
    ["draft", "Draft"],
    ["revised", "Revised"],
    ["stub_changed", "Stub changed"],
    ["upstream_changed", "Upstream changed"],
    ["settled", "Settled"],
  ] as const)("reads %s as %s", (state, word) => {
    expect(settleLabel(state)).toBe(word);
  });
});

describe("docOfPath", () => {
  it("finds a document by its repository path", () => {
    expect(docOfPath(fixture(), S02)?.title).toBe("Beta");
    expect(docOfPath(fixture(), "doc/product/demo/missing.md")).toBeUndefined();
  });
});

describe("frontierLinks", () => {
  function linkOf(gapOf: Gap) {
    const trace = fixture();
    trace.frontier = [gapOf];
    return frontierLinks(trace, "demo")[0]!;
  }

  it.each([
    [gap("revised", S02), "Beta changed since you settled it", "/f/demo/product/section_02"],
    [
      gap("stub_changed", FIRST),
      "First's stub changed since you settled it",
      "/f/demo/engineering/first",
    ],
    [
      gap("upstream_changed", FIRST),
      "The spec under First changed since you settled it",
      "/f/demo/engineering/first",
    ],
    [gap("settle", S01), "Read Alpha and settle it", "/f/demo/product/section_01"],
    [gap("settle", VISION), "Read Demo and settle it", "/f/demo/vision"],
    [gap("settle", METAPLAN), "Read Engineering and settle it", "/f/demo/metaplan"],
    [
      gap("revisit", FIRST, null, 1),
      "Revisit First before running it",
      "/f/demo/engineering/first",
    ],
    [
      gap("revisit", METAPLAN, "2-second-plan-secondmd", 1),
      "Revisit 2. Second (`PLAN-second.md`) before writing its plan",
      "/f/demo/metaplan#2-second-plan-secondmd",
    ],
    [
      gap("uncovered", S01, "two"),
      "Two is not covered by any plan",
      "/f/demo/source/product/demo/section_01.md#two",
    ],
    [gap("triage", "doc/plans/demo/findings.md", null, 1), "1 open finding", "/f/demo/findings"],
    [gap("triage", "doc/plans/demo/findings.md", null, 3), "3 open findings", "/f/demo/findings"],
  ])("words %j", (gapOf, text, to) => {
    expect(linkOf(gapOf)).toEqual({ text, to });
  });

  it("names a written plan reached through its metaplan entry", () => {
    const link = linkOf(gap("revisit", METAPLAN, "1-first-plan-firstmd", 1));
    expect(link).toEqual({
      text: "Revisit First before running it",
      to: "/f/demo/engineering/first",
    });
  });

  it("falls back to the file name for a document the trace does not hold", () => {
    expect(linkOf(gap("revised", "doc/product/demo/section_09.md")).text).toBe(
      "section_09.md changed since you settled it",
    );
  });

  it("keeps the server's order and stops at five", () => {
    const trace = fixture();
    trace.frontier = [S01, S02, S03, FIRST, VISION, METAPLAN, "doc/plans/demo/PLAN-second.md"].map(
      (path) => gap("settle", path),
    );
    const links = frontierLinks(trace, "demo");
    expect(links).toHaveLength(5);
    expect(links.map((link) => link.text)).toEqual([
      "Read Alpha and settle it",
      "Read Beta and settle it",
      "Read Gamma and settle it",
      "Read First and settle it",
      "Read Demo and settle it",
    ]);
  });

  it("is empty when the frontier is", () => {
    const trace = fixture();
    trace.frontier = [];
    expect(frontierLinks(trace, "demo")).toEqual([]);
  });
});

describe("chipsFor on a product page", () => {
  it("looks up the covers of the heading a sentence rests on", () => {
    const chips = chipsFor(fixture(), sentence([[S01, "one"]]), "product");
    expect(labels(chips)).toEqual(["entry:1. First (`PLAN-first.md`)", "plan:first"]);
    expect(chips[1]).toMatchObject({ path: FIRST, state: null, count: null });
  });

  it("reads an empty anchor as the whole section, with its stages and open findings", () => {
    const chips = chipsFor(fixture(), sentence([[S01, ""]]), "product");
    expect(labels(chips)).toEqual([
      "entry:1. First (`PLAN-first.md`)",
      "plan:first",
      "stage:first · build",
      "findings:1 open finding",
    ]);
    expect(chips.at(-1)).toMatchObject({ path: null, count: 1 });
  });

  it("counts only open findings", () => {
    const chips = chipsFor(fixture(), sentence([[S02, ""]]), "product");
    expect(labels(chips)).toEqual(["entry:2. Second (`PLAN-second.md`)"]);
  });

  it("lists a cover once when several headings share it", () => {
    const chips = chipsFor(
      fixture(),
      sentence([
        [S02, "three"],
        [S02, "four"],
      ]),
      "product",
    );
    expect(chips).toHaveLength(1);
  });

  it("is empty for a heading nothing covers or the trace does not hold", () => {
    expect(
      chipsFor(
        fixture(),
        sentence([
          [S03, ""],
          [S01, "missing"],
        ]),
        "product",
      ),
    ).toEqual([]);
  });
});

describe("chipsFor on a plan page", () => {
  it("reads the stage from a Source citation's stage anchor", () => {
    const chips = chipsFor(fixture(), sentence([], [[FIRST, "stage:build"]]), "plan");
    expect(chips).toEqual([
      {
        key: `spec:${S01}#two`,
        kind: "spec",
        label: "Two",
        path: S01,
        anchor: "two",
        state: "settled",
        count: null,
      },
    ]);
  });

  it("falls back to everything the plan implements when no stage is cited", () => {
    const chips = chipsFor(fixture(), sentence([[FIRST, "intro"]]), "plan");
    expect(labels(chips)).toEqual(["spec:One", "spec:Two"]);
  });

  it("carries the section's current settle state", () => {
    const trace = fixture();
    setSettle(trace, S01, "revised");
    const chips = chipsFor(trace, sentence([], [[FIRST, ""]]), "plan");
    expect(chips.map((chip) => chip.state)).toEqual(["revised", "revised"]);
  });

  it("is empty for a stage that implements nothing, and for a citation of no plan", () => {
    expect(chipsFor(fixture(), sentence([], [[FIRST, "stage:other"]]), "plan")).toEqual([]);
    expect(chipsFor(fixture(), sentence([], [[S01, "one"]]), "plan")).toEqual([]);
  });
});

describe("impactFor", () => {
  it("lists the plans covering a changed section with their states", () => {
    const trace = fixture();
    setSettle(trace, FIRST, "upstream_changed");
    expect(impactFor(trace, [S01])).toEqual([
      { path: S01, rows: [{ path: FIRST, title: "First", state: "upstream_changed" }] },
    ]);
  });

  it("lists a covering plan even while it is a draft", () => {
    expect(impactFor(fixture(), [S01])[0]!.rows.map((row) => row.state)).toEqual(["draft"]);
  });

  it("has no rows for a section nothing settled depends on", () => {
    expect(impactFor(fixture(), [S02, S03])).toEqual([
      { path: S02, rows: [] },
      { path: S03, rows: [] },
    ]);
  });

  it("lists sections whose stub row moved or that were revised when the vision changed", () => {
    const trace = fixture();
    setSettle(trace, S03, "stub_changed");
    expect(impactFor(trace, [VISION])[0]!.rows).toEqual([
      { path: S02, title: "Beta", state: "revised" },
      { path: S03, title: "Gamma", state: "stub_changed" },
    ]);
  });

  it("lists plans whose entry moved when the metaplan changed", () => {
    const trace = fixture();
    setSettle(trace, FIRST, "stub_changed");
    expect(impactFor(trace, [METAPLAN])[0]!.rows).toEqual([
      { path: FIRST, title: "First", state: "stub_changed" },
    ]);
  });

  it("lists the sections a changed plan implements when they moved", () => {
    const trace = fixture();
    setSettle(trace, S01, "revised");
    expect(impactFor(trace, [FIRST])[0]!.rows).toEqual([
      { path: S01, title: "Alpha", state: "revised" },
    ]);
  });

  it("follows a changed brief to its plan", () => {
    const trace = fixture();
    setSettle(trace, FIRST, "revised");
    expect(impactFor(trace, [BRIEF])).toEqual([
      { path: BRIEF, rows: [{ path: FIRST, title: "First", state: "revised" }] },
    ]);
  });

  it("never lists the changed document itself", () => {
    const trace = fixture();
    setSettle(trace, S01, "revised");
    expect(impactFor(trace, [S01])[0]!.rows.map((row) => row.path)).toEqual([FIRST]);
  });
});

describe("bundleLinks", () => {
  it("links each brief to its source page, labelled under the plan's folder", () => {
    expect(bundleLinks(fixture(), FIRST, "demo")).toEqual([
      {
        path: BRIEF,
        label: "build/r1.md",
        to: "/f/demo/source/plans/demo/briefs/first/build/r1.md",
      },
    ]);
  });

  it("is empty for a plan without briefs and for a path that is no document", () => {
    expect(bundleLinks(fixture(), "doc/plans/demo/PLAN-second.md", "demo")).toEqual([]);
    expect(bundleLinks(fixture(), "doc/plans/demo/PLAN-none.md", "demo")).toEqual([]);
  });
});
