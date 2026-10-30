import { cleanup, render } from "@testing-library/react";
import { CircleCheck, CircleDot, Minus, TriangleAlert } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { afterEach, describe, expect, it } from "vitest";

import {
  DOC_KIND_GLYPHS,
  OTHER_GLYPHS,
  PLAN_STATE_GLYPHS,
  STEP_DOC_KIND,
  STEP_GLYPHS,
  STEP_MARK_GLYPHS,
  SUMMARY_STATE_GLYPHS,
  VIEW_GLYPHS,
} from "@/plan/ui/glyphs";

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
    keys: ["home", "product", "engineering", "files"],
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

const glyphCases = [
  ...STEP_GLYPHS.map((glyph, index) => ({ name: `step ${index + 1}`, glyph })),
  ...glyphRecords.flatMap(({ name, glyphs }) =>
    Object.entries(glyphs).map(([key, glyph]) => ({ name: `${name}: ${key}`, glyph })),
  ),
];

function GlyphHarness({ glyph: Glyph }: { glyph: LucideIcon }) {
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

  it("provides six step glyphs and document kinds", () => {
    expect(STEP_GLYPHS).toHaveLength(6);
    expect(STEP_DOC_KIND).toHaveLength(6);
  });

  it("assigns document kinds in step order", () => {
    expect(STEP_DOC_KIND).toEqual(["vision", "section", "metaplan", "plan", "plan", "plan"]);
  });

  it.each(glyphCases)("renders $name as an SVG", ({ glyph }) => {
    const { container } = render(<GlyphHarness glyph={glyph} />);

    expect(container.querySelector("svg")).not.toBeNull();
  });

  it("uses a distinct glyph for each document kind", () => {
    const glyphs = Object.values(DOC_KIND_GLYPHS);

    expect(new Set(glyphs).size).toBe(glyphs.length);
  });
});

describe("step mark glyphs", () => {
  it("uses a circled check, circled dot, warning triangle and dash for step marks", () => {
    expect(STEP_MARK_GLYPHS).toEqual({
      done: CircleCheck,
      active: CircleDot,
      gap: TriangleAlert,
      optional: Minus,
    });
    expect(STEP_MARK_GLYPHS.done).toBe(OTHER_GLYPHS.stepDone);
  });

  it("keeps unfinished step marks distinct from document, plan and summary concepts", () => {
    const reserved = [
      ...Object.values(DOC_KIND_GLYPHS),
      ...Object.values(PLAN_STATE_GLYPHS),
      ...Object.values(SUMMARY_STATE_GLYPHS),
    ];

    for (const mark of ["active", "gap", "optional"] as const) {
      expect(reserved).not.toContain(STEP_MARK_GLYPHS[mark]);
    }
    expect(new Set(Object.values(STEP_MARK_GLYPHS)).size).toBe(4);
  });
});
