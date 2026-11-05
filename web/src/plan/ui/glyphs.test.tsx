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
import { RoundelGlyph } from "@/plan/ui/roundel-glyph";

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
// The states that mean work in progress draw the roundel, a disc of spans rather than an SVG
// (ui/DESIGN.md, Status); "draws the roundel for work in progress" below covers them.
const glyphCases = allCases.filter(({ glyph }) => glyph !== RoundelGlyph);
const roundelCases = allCases.filter(({ glyph }) => glyph === RoundelGlyph);

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

  it.each(glyphCases)("renders $name as an SVG", ({ glyph }) => {
    const { container } = render(<GlyphHarness glyph={glyph} />);

    expect(container.querySelector("svg")).not.toBeNull();
  });

  it("draws the roundel for work in progress: a running plan and a rebuilding summary", () => {
    expect(roundelCases.map(({ name }) => name)).toEqual([
      "plan states: running",
      "summary states: rebuilding",
    ]);
    for (const { glyph } of roundelCases) {
      const { container } = render(<GlyphHarness glyph={glyph} />);

      expect(container.querySelector("svg")).toBeNull();
      expect(
        container.querySelector('[aria-hidden="true"] .aurora-roundel')?.getAttribute("data-busy"),
      ).toBe("true");
      cleanup();
    }
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
