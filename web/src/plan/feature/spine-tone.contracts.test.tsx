import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { checklistViewSchema, serverInfoSchema, type ChecklistView } from "@/plan/api/schema";
import { Spine } from "@/plan/feature/spine";
import spineSheet from "@/plan/feature/spine.css?raw";
import { projectAtom, serverInfoAtom } from "@/plan/state/atoms";

beforeEach(() => window.localStorage.clear());
afterEach(cleanup);

const section = (number: string, written: boolean, name = `Section ${number}`) => ({
  number,
  name,
  written,
  open_questions: 0,
});

const plan = (slug: string, state: string, pressure_date: string | null = null) => ({
  slug,
  path: `doc/plans/atlas/PLAN-${slug}.md`,
  title: `Plan ${slug}`,
  state,
  pressure_date,
  listed: true,
});

function atlas(): ChecklistView {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    sections: [section("01", true, "The `Run` loop"), section("02", false)],
    plan_rows: [
      plan("atlas-01", "done", "2026-10-20"),
      plan("atlas-02", "running"),
      plan("atlas-03", "written"),
      plan("atlas-04", "done"),
    ],
  });
}

function renderSpine(path: string, checklist: ChecklistView = atlas()) {
  const store = createStore();
  store.set(serverInfoAtom, serverInfoSchema.parse(serverInfoFixture));
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  const router = createMemoryRouter([{ path: "/f/:feature/*", element: <Spine /> }], {
    initialEntries: [path],
  });
  return render(
    <Provider store={store}>
      <div data-loom-plan="">
        <RouterProvider router={router} />
      </div>
    </Provider>,
  );
}

/** Every comma-split selector of the spine.css rules that set the completed tone as the text colour. */
function completedToneSelectors(): string[] {
  const bare = spineSheet.replace(/\/\*[\s\S]*?\*\//g, "");
  return Array.from(bare.matchAll(/([^{}]+)\{([^{}]*)\}/g))
    .filter((block) => /(?:^|;)\s*color\s*:\s*var\(--tone-completed\)/.test(block[2]))
    .flatMap((block) => block[1].split(",").map((entry) => entry.trim()));
}

function lit(element: Element | null, selectors: string[]): boolean {
  if (element === null) throw new Error("the glyph is missing");
  return selectors.some((selector) => {
    try {
      return element.matches(selector);
    } catch {
      return false;
    }
  });
}

it("a done plan glyph and a tested plan glyph take the completed tone", () => {
  renderSpine("/f/atlas/engineering");
  const nav = screen.getByRole("navigation", { name: "Feature documents" });
  const link = (slug: string) => within(nav).getByRole("link", { name: new RegExp(`^${slug}`) });
  const stateGlyph = (slug: string) => link(slug).querySelector(".plan-spine__glyph");
  const badgeGlyph = (slug: string) =>
    link(slug).querySelector(".plan-spine__badge .plan-spine__glyph");

  const states = { "atlas-01": "done", "atlas-02": "running", "atlas-03": "written" };
  for (const [slug, state] of Object.entries({ ...states, "atlas-04": "done" })) {
    expect(stateGlyph(slug)?.getAttribute("data-state"), slug).toBe(state);
  }

  const selectors = completedToneSelectors();
  expect(lit(stateGlyph("atlas-01"), selectors)).toBe(true);
  expect(lit(stateGlyph("atlas-04"), selectors)).toBe(true);
  expect(lit(badgeGlyph("atlas-01"), selectors)).toBe(true);
  expect(lit(stateGlyph("atlas-02"), selectors)).toBe(false);
  expect(lit(stateGlyph("atlas-03"), selectors)).toBe(false);
  expect(lit(badgeGlyph("atlas-02"), selectors)).toBe(false);
});
