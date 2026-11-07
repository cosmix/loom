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

/** The spine's glyph of `kind`, placed under the plan root so the stylesheet's selectors can match it. */
function glyphOf(kind: string): Element {
  const root = document.createElement("div");
  root.setAttribute("data-loom-plan", "");
  const glyph = document.createElement("span");
  glyph.className = "plan-spine__glyph";
  glyph.setAttribute("data-glyph", kind);
  root.append(glyph);
  document.body.append(root);
  return glyph;
}

it("a done plan glyph and a settled glyph take the completed tone", () => {
  renderSpine("/f/atlas/engineering");
  const nav = screen.getByRole("navigation", { name: "Feature documents" });
  const kindOf = (slug: string) =>
    within(nav).getByTitle(slug).querySelector(".plan-spine__glyph")?.getAttribute("data-glyph");

  expect(kindOf("atlas-01")).toBe("done");
  expect(kindOf("atlas-02")).toBe("running");
  expect(kindOf("atlas-03")).toBe("draft");
  expect(kindOf("atlas-04")).toBe("done");

  const selectors = completedToneSelectors();
  for (const kind of ["done", "settled"]) expect(lit(glyphOf(kind), selectors), kind).toBe(true);
  for (const kind of ["running", "attention", "not-written", "draft", "retired"]) {
    expect(lit(glyphOf(kind), selectors), kind).toBe(false);
  }
});
