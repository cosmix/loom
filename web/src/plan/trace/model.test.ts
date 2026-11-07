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
  docOfPath,
  frontierHead,
  impactFor,
  shortSlug,
  shownSettle,
  withoutNumber,
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

describe("withoutNumber", () => {
  it("drops the number a title opens with when it is the shown one", () => {
    expect(withoutNumber("04 — Summary Engine", "04")).toBe("Summary Engine");
    expect(withoutNumber("6. File Mode", 6)).toBe("File Mode");
    expect(withoutNumber("06: The store", "06")).toBe("The store");
  });

  it("keeps a title that opens with another number or with none", () => {
    expect(withoutNumber("04 — Summary Engine", "05")).toBe("04 — Summary Engine");
    expect(withoutNumber("Summary Engine", "04")).toBe("Summary Engine");
    expect(withoutNumber("2026 roadmap", "04")).toBe("2026 roadmap");
  });
});

describe("shortSlug", () => {
  it("strips the feature prefix and leaves any other slug whole", () => {
    expect(shortSlug("feat-planner", "feat-planner-05-trace")).toBe("05-trace");
    expect(shortSlug("feat-planner", "other-05-trace")).toBe("other-05-trace");
    expect(shortSlug("feat-planner", "feat-planner")).toBe("feat-planner");
    expect(shortSlug("feat-planner", "feat-planner-")).toBe("feat-planner-");
  });
});

describe("docOfPath", () => {
  it("finds a document by its repository path", () => {
    expect(docOfPath(fixture(), S02)?.title).toBe("Beta");
    expect(docOfPath(fixture(), "doc/product/demo/missing.md")).toBeUndefined();
  });
});

describe("shownSettle", () => {
  it("shows the state of a settleable document and none for done work, a stub or nothing", () => {
    const trace = fixture();
    expect(shownSettle(docOfPath(trace, S02))).toBe("revised");
    // The fixture's section 03 is a stub, which the server marks unsettleable.
    expect(shownSettle(docOfPath(trace, S03))).toBeNull();
    docOfPath(trace, S02)!.settleable = false;
    expect(shownSettle(docOfPath(trace, S02))).toBeNull();
    expect(shownSettle(undefined)).toBeNull();
  });
});

describe("frontierHead", () => {
  function headOf(gapOf: Gap, edit: (trace: TraceView) => void = () => {}) {
    const trace = fixture();
    trace.frontier = [gapOf];
    edit(trace);
    return frontierHead(trace, "demo");
  }

  it.each([
    [
      gap("revised", S02),
      "Beta changed since you settled it.",
      "Review Beta",
      "/f/demo/product/section_02",
    ],
    [
      gap("stub_changed", FIRST),
      "First's stub changed since you settled it.",
      "Review First",
      "/f/demo/engineering/first",
    ],
    [
      gap("upstream_changed", FIRST),
      "The spec under first changed since you settled it.",
      "Review first",
      "/f/demo/engineering/first",
    ],
    [gap("settle", S01), "Read Alpha and settle it.", "Read Alpha", "/f/demo/product/section_01"],
    [gap("settle", VISION), "Read Demo and settle it.", "Read Demo", "/f/demo/vision"],
    [
      gap("revisit", METAPLAN, "2-second-plan-secondmd", 1),
      "Revisit 2. Second (`PLAN-second.md`) before writing its plan.",
      "Open the metaplan",
      "/f/demo/metaplan#2-second-plan-secondmd",
    ],
    [
      gap("revisit", FIRST, null, 1),
      "Revisit First before running it.",
      "Open first",
      "/f/demo/engineering/first",
    ],
    [
      gap("uncovered", S01, "two"),
      "Two is not covered by any plan.",
      "Open coverage",
      "/f/demo/coverage#section_01%3Atwo",
    ],
    [
      gap("triage", "doc/plans/demo/findings.md", null, 1),
      "One finding from the last run is waiting for you.",
      "Triage findings",
      "/f/demo/findings",
    ],
    [
      gap("triage", "doc/plans/demo/findings.md", null, null),
      "Findings from the last run are waiting for you.",
      "Triage findings",
      "/f/demo/findings",
    ],
    [
      gap("triage", "doc/plans/demo/findings.md", null, 3),
      "Three findings from the last run are waiting for you.",
      "Triage findings",
      "/f/demo/findings",
    ],
    [
      gap("triage", "doc/plans/demo/findings.md", null, 21),
      "21 findings from the last run are waiting for you.",
      "Triage findings",
      "/f/demo/findings",
    ],
  ])("words %j", (gapOf, sentence, label, to) => {
    expect(headOf(gapOf)).toEqual({ sentence, detail: null, label, to });
  });

  it("names a written plan reached through its metaplan entry", () => {
    expect(headOf(gap("revisit", METAPLAN, "1-first-plan-firstmd", 1))).toMatchObject({
      sentence: "Revisit First before running it.",
      to: "/f/demo/engineering/first",
    });
  });

  it("falls back to the file name for a document the trace does not hold", () => {
    expect(headOf(gap("revised", "doc/product/demo/section_09.md"))?.sentence).toBe(
      "section_09.md changed since you settled it.",
    );
  });

  it("says which settled plan a revised section flags, by its short slug", () => {
    // Plan first covers section 01's headings; the fixture's plan slug is `first`.
    const head = headOf(gap("revised", S01), (trace) => {
      docOfPath(trace, FIRST)!.settle = "upstream_changed";
      docOfPath(trace, FIRST)!.settleable = true;
      docOfPath(trace, FIRST)!.slug = "demo-05-trace";
    });
    expect(head?.detail).toBe("Plan 05-trace implements it, so it is flagged too.");
  });

  it("adds no detail when no settled plan is flagged", () => {
    expect(headOf(gap("revised", S01))?.detail).toBeNull();
  });

  it("reads only the first gap, and nothing when the frontier is empty", () => {
    const trace = fixture();
    expect(frontierHead(trace, "demo")?.sentence).toBe("Beta changed since you settled it.");
    trace.frontier = [];
    expect(frontierHead(trace, "demo")).toBeNull();
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
    // Section 03 written and settled before its vision row moved; the fixture lists it unwritten.
    Object.assign(docOfPath(trace, S03)!, { progress: "written", settleable: true });
    setSettle(trace, S03, "stub_changed");
    expect(impactFor(trace, [VISION])[0]!.rows).toEqual([
      { path: S02, title: "Beta", state: "revised" },
      { path: S03, title: "Gamma", state: "stub_changed" },
    ]);
  });

  it("leaves out a related document that moved but is not asked to be settled", () => {
    const trace = fixture();
    setSettle(trace, S03, "stub_changed");
    expect(impactFor(trace, [VISION])[0]!.rows).toEqual([
      { path: S02, title: "Beta", state: "revised" },
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
