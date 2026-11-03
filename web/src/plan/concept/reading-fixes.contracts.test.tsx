import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import runsSheet from "@/plan/concept/concept-runs.css?raw";

import {
  clone,
  makeStubFetch,
  openFirstRun,
  OVERVIEW_RUN,
  readingColumn,
  renderAt,
  type Page,
} from "./concept-route-support";

/*
 * Contracts for the reading-and-summaries stage of PLAN-planner-issue-fixes:
 *   - #8: the summary and overview header button reads "Open all" while a sentence is closed,
 *     "Fold all" once every one is open, and Fold all folds the page at every level;
 *   - #2: an open source-passage block always draws a 2px edge in its document's hue.
 */

let pages: Record<string, unknown> = {};
const fetched: string[] = [];
const DETAIL = /^Detail of/;
const NESTED_MEMBER = /^A plan starts from a short statement of intent/;

function serveOverview(overview: Page = clone(overviewFixture)): void {
  pages = {
    "overview/demo/product": { ...overview, key: "overview/demo/product" },
    "overview/demo/engineering": { ...overview, key: "overview/demo/engineering" },
    "demo/product/section_01": clone(summaryFixture),
    "demo/product/section_02": {
      ...clone(summaryFixture),
      key: "demo/product/section_02",
      title: "Review",
    },
  };
}

beforeEach(() => {
  pages = {};
  fetched.length = 0;
  window.sessionStorage.clear();
  vi.stubGlobal(
    "fetch",
    vi.fn(makeStubFetch(() => ({ pages, docs: {}, failing: false }), fetched)),
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/** A pointer click on a header button: the button takes focus, then the click fires. */
function press(button: HTMLElement): void {
  act(() => button.focus());
  fireEvent.click(button);
}

const runsIn = (column: HTMLElement) => within(column).queryAllByRole("group", { name: DETAIL });

/** Fold all on the page, then: no run at any level, the label is back, focus stayed on it. */
async function foldAllAndExpectFolded(column: HTMLElement): Promise<void> {
  press(await screen.findByRole("button", { name: "Fold all" }));
  await waitFor(() => expect(runsIn(column)).toHaveLength(0));
  const button = await screen.findByRole("button", { name: "Open all" });
  expect(screen.queryByRole("button", { name: "Fold all" })).toBeNull();
  expect(document.activeElement).toBe(button);
}

it("Open all becomes Fold all and folds the page", async () => {
  serveOverview();
  renderAt("/f/demo/product/section_01");
  const summary = await readingColumn();
  expect(runsIn(summary)).toHaveLength(0);
  press(await screen.findByRole("button", { name: "Open all" }));
  expect(await within(summary).findAllByRole("group", { name: DETAIL })).not.toHaveLength(0);
  expect(await screen.findByRole("button", { name: "Fold all" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Open all" })).toBeNull();
  await foldAllAndExpectFolded(summary);

  cleanup();
  window.sessionStorage.clear();
  serveOverview();
  renderAt("/f/demo/product");
  const overview = await openFirstRun();
  const outer = within(overview).getByRole("group", { name: OVERVIEW_RUN });
  fireEvent.click(await within(outer).findByRole("button", { name: NESTED_MEMBER }));
  await waitFor(() =>
    expect(within(outer).queryAllByRole("group", { name: DETAIL })).not.toHaveLength(0),
  );
  press(await screen.findByRole("button", { name: "Open all" }));
  await foldAllAndExpectFolded(overview);
});

/** Every declaration of every rule whose selector list names `selector` on its own. */
function declarationsOf(sheet: string, selector: string): string[] {
  const css = sheet.replace(/\/\*[\s\S]*?\*\//g, "");
  const found: string[] = [];
  for (const [, selectors = "", body = ""] of css.matchAll(/([^{};]+)\{([^{}]*)\}/g)) {
    if (!selectors.split(",").some((part) => part.trim() === selector)) continue;
    for (const decl of body.split(";")) {
      const normal = decl.replace(/\s+/g, " ").trim().toLowerCase();
      if (normal) found.push(normal.replace(/\s*:\s*/, ": "));
    }
  }
  return found;
}

it("an open block draws a 2px edge in its document hue", () => {
  const declarations = declarationsOf(runsSheet, ".concept-passages");
  expect(declarations).toContain("border-inline-start: 2px solid var(--run-colour)");
  const css = runsSheet.replace(/\/\*[\s\S]*?\*\//g, "");
  // The hover-only hairline: the edge (or its colour) read from --run-edge anywhere in the sheet.
  expect(css).not.toMatch(/border-inline-start(?:-color)?\s*:[^;{}]*var\(\s*--run-edge\s*\)/);
});
