import { cleanup, render } from "@testing-library/react";
import { CircleCheck, CircleDashed, CircleDot, Minus } from "lucide-react";
import { afterEach, describe, expect, it } from "vitest";

import {
  DOC_KIND_GLYPHS,
  OTHER_GLYPHS,
  PLAN_STATE_GLYPHS,
  STEP_MARK_GLYPHS,
  SUMMARY_STATE_GLYPHS,
  VIEW_GLYPHS,
  type Glyph,
} from "@/plan/ui/glyphs";
import { LiveDot } from "@/plan/ui/activity-glyphs";
import { ArrowSpinner } from "@/plan/ui/spinner";

afterEach(cleanup);

const glyphRecords = [
  {
    name: "document kinds",
    glyphs: DOC_KIND_GLYPHS,
    keys: ["vision", "section", "note", "metaplan", "plan", "brief", "knowledge", "review"],
  },
  {
    name: "plan states",
    glyphs: PLAN_STATE_GLYPHS,
    keys: ["written", "running", "done", "retired"],
  },
  {
    name: "summary states",
    glyphs: SUMMARY_STATE_GLYPHS,
    keys: ["current", "rebuilding", "stale"],
  },
  {
    name: "step marks",
    glyphs: STEP_MARK_GLYPHS,
    keys: ["done", "active", "gap", "optional"],
  },
  {
    name: "views",
    glyphs: VIEW_GLYPHS,
    keys: ["home", "files"],
  },
  {
    name: "other concepts",
    glyphs: OTHER_GLYPHS,
    keys: [
      "readOnly",
      "pressureTested",
      "openQuestions",
      "stepDone",
      "switcher",
      "settings",
      "help",
    ],
  },
];

const allCases = glyphRecords.flatMap(({ name, glyphs }) =>
  Object.entries(glyphs).map(([key, glyph]) => ({ name: `${name}: ${key}`, glyph })),
);
// A running plan is a state shown, the live dot (a span); a rebuilding summary is an operation, the
// arrow spinner (ui/DESIGN.md, Status). The tests below the SVG one cover both.
const svgCases = allCases.filter(({ glyph }) => glyph !== LiveDot);

function GlyphHarness({ glyph: Glyph }: { glyph: Glyph }) {
  return (
    <span>
      <Glyph aria-hidden="true" />
    </span>
  );
}

describe("planner glyphs", () => {
  it.each(glyphRecords)("covers every key for $name", ({ glyphs, keys }) => {
    expect(Object.keys(glyphs).sort()).toEqual([...keys].sort());
  });

  it.each(svgCases)("renders $name as an SVG", ({ glyph }) => {
    const { container } = render(<GlyphHarness glyph={glyph} />);

    expect(container.querySelector("svg")).not.toBeNull();
  });

  it("draws a running plan as the live status dot and a rebuild as the arrow spinner", () => {
    expect(PLAN_STATE_GLYPHS.running).toBe(LiveDot);
    expect(SUMMARY_STATE_GLYPHS.rebuilding).toBe(ArrowSpinner);
    expect(allCases.filter(({ glyph }) => glyph === LiveDot || glyph === ArrowSpinner)).toHaveLength(
      2,
    );

    const running = render(<GlyphHarness glyph={PLAN_STATE_GLYPHS.running} />);
    expect(running.container.querySelector('[aria-hidden="true"] .activity-glyph__led')).not.toBeNull();
    expect(running.container.querySelector("svg")).toBeNull();
    cleanup();
    const rebuilding = render(<GlyphHarness glyph={SUMMARY_STATE_GLYPHS.rebuilding} />);
    const svg = rebuilding.container.querySelector("svg.plan-arrow-spinner");
    expect(svg?.getAttribute("aria-hidden")).toBe("true");
    expect(svg?.getAttribute("data-busy")).toBe("true");
    expect(rebuilding.container.querySelector(".aurora-roundel")).toBeNull();
  });

  it("uses a distinct glyph for each document kind", () => {
    const glyphs = Object.values(DOC_KIND_GLYPHS);

    expect(new Set(glyphs).size).toBe(glyphs.length);
  });
});

describe("step mark glyphs", () => {
  it("uses a circled check, circled dot, dashed circle and dash for step marks", () => {
    expect(STEP_MARK_GLYPHS).toEqual({
      done: CircleCheck,
      active: CircleDot,
      gap: CircleDashed,
      optional: Minus,
    });
    expect(STEP_MARK_GLYPHS.done).toBe(OTHER_GLYPHS.stepDone);
  });

  it("keeps unfinished step marks distinct from document and plan concepts", () => {
    const reserved = [...Object.values(DOC_KIND_GLYPHS), ...Object.values(PLAN_STATE_GLYPHS)];

    for (const mark of ["active", "gap", "optional"] as const) {
      expect(reserved).not.toContain(STEP_MARK_GLYPHS[mark]);
    }
    // A gap and an out-of-date summary share the dashed circle: both are an unfinished document.
    expect(STEP_MARK_GLYPHS.gap).toBe(SUMMARY_STATE_GLYPHS.stale);
    for (const mark of ["active", "optional"] as const) {
      expect(Object.values(SUMMARY_STATE_GLYPHS)).not.toContain(STEP_MARK_GLYPHS[mark]);
    }
    expect(new Set(Object.values(STEP_MARK_GLYPHS)).size).toBe(4);
  });
});
