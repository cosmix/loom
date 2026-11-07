import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it } from "vitest";

import {
  DOC_KIND_GLYPHS,
  OTHER_GLYPHS,
  PLAN_STATE_GLYPHS,
  STEP_MARK_GLYPHS,
  SUMMARY_STATE_GLYPHS,
  type DocKind,
  type PlanState,
  type SummaryState,
} from "../ui/glyphs";
import { PlannerLegend } from "./legend";

const READING_NAMES = [
  "Source mark",
  "Dashed mark",
  "No source",
  "Fold cap",
  "Plan link",
  "Read-only",
  "Pressure-test status",
  "Open questions",
  "Feature switcher",
  "Settings",
  "Help",
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

const STEP_NAMES = [
  "Done step",
  "Step with gaps",
  "Active step",
  "Optional step",
  "Finished pressure step",
] as const;

const STATE_NAMES = [
  ...Object.values(DOCUMENT_NAMES),
  ...Object.values(PLAN_NAMES),
  ...Object.values(SUMMARY_NAMES),
  ...STEP_NAMES,
];

/** The marks of the spine's Coverage and Findings wells, each drawn with the well's own dot. */
const WELL_DOTS = [
  ["Covered heading", "covered"],
  ["Uncovered heading", "uncovered"],
  ["Open finding", "open"],
  ["Decided finding", "decided"],
] as const;

/** One sign of work in progress, then the two that mean "your turn". */
const PROGRESS_NAMES = ["Working", "Needs permission", "Waiting for you"] as const;

afterEach(cleanup);

/** The name of a legend term: its text without the aria-hidden glyph tile. */
function termName(term: HTMLElement): string {
  const clone = term.cloneNode(true) as HTMLElement;
  for (const tile of clone.querySelectorAll('[aria-hidden="true"]')) tile.remove();
  return clone.textContent ?? "";
}

function tileOf(group: HTMLElement, name: string): HTMLElement {
  const term = within(group)
    .getAllByRole("term")
    .find((candidate) => termName(candidate) === name);
  const tile = term?.querySelector<HTMLElement>('[aria-hidden="true"]');
  if (!tile) throw new Error(`no legend tile for ${name}`);
  return tile;
}

it("groups its definitions under the four headings", () => {
  render(<PlannerLegend />, { wrapper: MemoryRouter });

  expect(screen.getAllByRole("heading").map((heading) => heading.textContent)).toEqual([
    "Reading",
    "States",
    "Spine wells",
    "In progress",
  ]);
  for (const title of ["Reading", "States", "Spine wells", "In progress"]) {
    expect(screen.getByRole("region", { name: title })).toBeDefined();
  }
});

it("lists each group's entries in order, once, with a meaning", () => {
  render(<PlannerLegend />, { wrapper: MemoryRouter });

  const groups: [string, readonly string[]][] = [
    ["Reading", READING_NAMES],
    ["States", STATE_NAMES],
    ["Spine wells", WELL_DOTS.map(([name]) => name)],
    ["In progress", PROGRESS_NAMES],
  ];
  for (const [title, names] of groups) {
    const group = screen.getByRole("region", { name: title });
    expect(within(group).getAllByRole("term").map(termName)).toEqual(names);
    const meanings = within(group).getAllByRole("definition");
    expect(meanings).toHaveLength(names.length);
    for (const meaning of meanings) {
      expect(meaning.textContent?.trim().length).toBeGreaterThan(0);
    }
  }

  const all = screen.getAllByRole("term").map(termName);
  expect(new Set(all).size).toBe(all.length);
  expect(STATE_NAMES).toHaveLength(
    Object.keys(DOC_KIND_GLYPHS).length +
      Object.keys(PLAN_STATE_GLYPHS).length +
      Object.keys(SUMMARY_STATE_GLYPHS).length +
      STEP_NAMES.length,
  );
});

it("draws the reading marks with the reading column's own components", () => {
  render(<PlannerLegend />, { wrapper: MemoryRouter });
  const reading = screen.getByRole("region", { name: "Reading" });

  const source = tileOf(reading, "Source mark");
  expect(source.querySelectorAll(".concept-mark")).toHaveLength(1);
  expect(source.querySelector(".concept-mark[data-dashed]")).toBeNull();

  const dashed = tileOf(reading, "Dashed mark");
  expect(dashed.querySelectorAll('.concept-mark[data-dashed="true"]')).toHaveLength(1);

  expect(tileOf(reading, "No source").querySelector(".concept-no-source")?.textContent).toBe(
    "no source",
  );

  const cap = tileOf(reading, "Fold cap");
  expect(cap.querySelectorAll("button.concept-run-cap[aria-label='Fold']")).toHaveLength(1);

  expect(
    tileOf(reading, "Plan link").querySelectorAll(
      'button.concept-plan-glyph[aria-label="Plans implementing this section"]',
    ),
  ).toHaveLength(1);

  for (const name of ["Read-only", "Pressure-test status"]) {
    expect(tileOf(reading, name).querySelector("svg")).not.toBeNull();
  }
});

it("draws each spine mark with its glyph and the pressure timeline's check", () => {
  render(<PlannerLegend />, { wrapper: MemoryRouter });
  const states = screen.getByRole("region", { name: "States" });

  for (const [name, mark] of [
    ["Done step", "done"],
    ["Step with gaps", "gap"],
    ["Active step", "active"],
    ["Optional step", "optional"],
  ] as const) {
    const tile = tileOf(states, name);
    expect(tile.querySelector(`[data-step-mark="${mark}"]`)).not.toBeNull();
    // The spine states its marks as glyphs, so no progress strip stands beside them.
    expect(tile.querySelector('ol[aria-label="Progress"]')).toBeNull();
  }
  expect(
    tileOf(states, "Finished pressure step").querySelector('[data-other-glyph="stepDone"]'),
  ).not.toBeNull();
});

it("draws each well mark with the spine well's own dot class and state", () => {
  render(<PlannerLegend />, { wrapper: MemoryRouter });
  const wells = screen.getByRole("region", { name: "Spine wells" });

  for (const [name, dot] of WELL_DOTS) {
    const dots = tileOf(wells, name).querySelectorAll(".plan-spine__instrument-dot");
    expect(dots, name).toHaveLength(1);
    expect(dots[0]!.getAttribute("data-dot"), name).toBe(dot);
  }
});

it("sizes every legend glyph tile at 40px high", () => {
  const { container } = render(<PlannerLegend />, { wrapper: MemoryRouter });

  const tiles = container.querySelectorAll('dt > [aria-hidden="true"]');
  expect(tiles.length).toBe(screen.getAllByRole("term").length);
  for (const tile of tiles) expect(tile.className).toContain("h-10");
});

it("documents every other-concept and step-mark glyph exactly once", () => {
  const { container } = render(<PlannerLegend />, { wrapper: MemoryRouter });

  const groups = [
    ["data-other-glyph", OTHER_GLYPHS],
    ["data-step-mark", STEP_MARK_GLYPHS],
  ] as const;
  for (const [attribute, glyphs] of groups) {
    for (const key of Object.keys(glyphs)) {
      expect(container.querySelectorAll(`[${attribute}="${key}"]`)).toHaveLength(1);
    }
  }
});

it("uses each production document, state and activity glyph", () => {
  render(<PlannerLegend />, { wrapper: MemoryRouter });
  const states = screen.getByRole("region", { name: "States" });
  for (const name of [
    ...Object.values(DOCUMENT_NAMES),
    ...Object.values(PLAN_NAMES),
    ...Object.values(SUMMARY_NAMES),
  ]) {
    expect(tileOf(states, name).querySelector("svg, .activity-glyph")).not.toBeNull();
  }

  const group = screen.getByRole("region", { name: "In progress" });
  expect(within(group).getAllByRole("term")).toHaveLength(PROGRESS_NAMES.length);
  expect(
    tileOf(group, "Needs permission").querySelectorAll(
      '.activity-glyph[data-kind="permission"] svg',
    ),
  ).toHaveLength(1);
  expect(
    tileOf(group, "Waiting for you").querySelectorAll(
      '.activity-glyph[data-kind="waiting"] .activity-glyph__led',
    ),
  ).toHaveLength(1);
});

it("shows a state word's mark as the two rotating arrows, and no roundel", () => {
  const { container } = render(<PlannerLegend />, { wrapper: MemoryRouter });
  const group = screen.getByRole("region", { name: "In progress" });

  const working = tileOf(group, "Working");
  expect(working.querySelectorAll(".lucide-refresh-cw.plan-arrow-spinner")).toHaveLength(1);
  // Working, a running plan and a rebuilding summary: every state word is the same arrows.
  expect(container.querySelectorAll(".lucide-refresh-cw.plan-arrow-spinner")).toHaveLength(3);
  // The roundel is the header pill's alone; no single-arrow icon stands in for the arrows.
  expect(container.querySelectorAll(".aurora-roundel")).toHaveLength(0);
  expect(group.querySelectorAll(".lucide-loader-circle, .lucide-rotate-cw")).toHaveLength(0);
});

it("sets no legend text below 13px", () => {
  const { container } = render(<PlannerLegend />, { wrapper: MemoryRouter });

  expect(container.innerHTML).not.toMatch(/text-\[1[0-2]px\]/);
  for (const meaning of screen.getAllByRole("definition")) {
    expect(meaning.className).toContain("text-[13px]");
  }
  // No production text-xs markup remains in the legend.
  expect(container.querySelectorAll(".text-xs")).toHaveLength(0);
});
