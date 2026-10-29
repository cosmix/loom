import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";

import { ACTIVITY_KINDS, ACTIVITY_LABELS } from "../ui/activity-glyphs";
import {
  DOC_KIND_GLYPHS,
  PLAN_STATE_GLYPHS,
  SUMMARY_STATE_GLYPHS,
  type DocKind,
  type PlanState,
  type SummaryState,
} from "../ui/glyphs";
import { PlannerLegend } from "./legend";

const READING_NAMES = [
  "Source mark",
  "Dashed mark",
  "Fold cap",
  "Read-only",
  "Pressure-tested",
  "Open questions",
] as const;

const DOCUMENT_NAMES: Record<DocKind, string> = {
  vision: "Vision",
  section: "Section",
  note: "Note",
  metaplan: "Metaplan",
  plan: "Plan",
  brief: "Brief",
  knowledge: "Knowledge",
  review: "Run review",
};

const PLAN_NAMES: Record<PlanState, string> = {
  written: "Written",
  running: "Running",
  done: "Done",
  retired: "Retired",
};

const SUMMARY_NAMES: Record<SummaryState, string> = {
  current: "Current",
  rebuilding: "Rebuilding",
  stale: "Stale",
};

afterEach(cleanup);

it("groups its definitions under the three headings", () => {
  render(<PlannerLegend />);

  expect(screen.getAllByRole("heading").map((heading) => heading.textContent)).toEqual([
    "Reading",
    "States",
    "In progress",
  ]);
  for (const title of ["Reading", "States", "In progress"]) {
    expect(screen.getByRole("region", { name: title })).toBeDefined();
  }
});

it("names every reading mark, document kind, state and activity exactly once", () => {
  render(<PlannerLegend />);

  const expectedNames = [
    ...READING_NAMES,
    ...Object.values(DOCUMENT_NAMES),
    ...Object.values(PLAN_NAMES),
    ...Object.values(SUMMARY_NAMES),
    ...ACTIVITY_KINDS.map((kind) => ACTIVITY_LABELS[kind]),
  ];
  const names = screen.getAllByRole("term").map((term) => term.textContent);

  expect(names).toEqual(expectedNames);
  expect(new Set(names).size).toBe(names.length);
  expect(names).toHaveLength(
    READING_NAMES.length +
      Object.keys(DOC_KIND_GLYPHS).length +
      Object.keys(PLAN_STATE_GLYPHS).length +
      Object.keys(SUMMARY_STATE_GLYPHS).length +
      ACTIVITY_KINDS.length,
  );
  for (const name of expectedNames) {
    expect(screen.getAllByText(name, { exact: true })).toHaveLength(1);
  }
  expect(screen.getAllByRole("definition")).toHaveLength(names.length);
  for (const definition of screen.getAllByRole("definition")) {
    expect(definition.textContent?.trim().length).toBeGreaterThan(0);
  }
});

it("uses each production activity glyph with its shared label and meaning", () => {
  render(<PlannerLegend />);

  const group = screen.getByRole("region", { name: "In progress" });
  expect(within(group).getAllByRole("term")).toHaveLength(ACTIVITY_KINDS.length);
  for (const kind of ACTIVITY_KINDS) {
    const term = within(group).getByText(ACTIVITY_LABELS[kind], { exact: true });
    expect(term.querySelectorAll(`.activity-glyph[data-kind="${kind}"]`)).toHaveLength(1);
    expect(term.parentElement?.querySelector("dd")?.textContent).toBeTruthy();
  }
});
