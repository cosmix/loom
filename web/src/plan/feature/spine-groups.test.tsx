import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

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

const plan = (slug: string, state: string) => ({
  slug,
  path: `doc/plans/atlas/PLAN-${slug}.md`,
  title: `Plan ${slug}`,
  state,
  pressure_date: null,
  listed: true,
});

function atlas(patch: Record<string, unknown> = {}): ChecklistView {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    sections: [section("01", true, "The `Run` loop"), section("02", false)],
    plan_rows: [plan("atlas-01", "done"), plan("atlas-02", "running")],
    ...patch,
  });
}

function renderSpine(path: string, checklist: ChecklistView = atlas()) {
  const store = createStore();
  store.set(serverInfoAtom, serverInfoSchema.parse(serverInfoFixture));
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  const router = createMemoryRouter([{ path: "/f/:feature/*", element: <Spine /> }], {
    initialEntries: [path],
  });
  const view = render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { ...view, router, store };
}

const spine = () => screen.getByRole("navigation", { name: "Feature documents" });
/** The row's text with its `.sr-only` spans taken out: what a sighted reader sees. */
const visibleText = (row: Element) => {
  const copy = row.cloneNode(true) as Element;
  for (const hidden of copy.querySelectorAll(".sr-only")) hidden.remove();
  return copy.textContent?.replace(/\s+/g, " ").trim();
};
const link = (name: string | RegExp) => within(spine()).getByRole("link", { name });
/** A plan's row: the spine shortens the slug and keeps the full one in the row's `title`. */
const planRow = (slug: string) => within(spine()).getByTitle(slug);

describe("hostile names and marks", () => {
  it("renders for a feature named like an Object.prototype member", () => {
    renderSpine("/f/constructor/product", atlas({ feature: "constructor" }));

    expect(link(/\bPlans\b/)).toBeTruthy();
    fireEvent.click(within(spine()).getByRole("button", { name: "Expand Plans" }));
    expect(link(/atlas-01/)).toBeTruthy();
  });

  it("draws a plan whose state names an Object.prototype member as a plain draft glyph", () => {
    renderSpine("/f/atlas/engineering", atlas({ plan_rows: [plan("atlas-03", "constructor")] }));

    const glyphs = planRow("atlas-03").querySelectorAll(".plan-spine__glyph");
    // An unknown state falls back to the draft glyph; the state string is never an icon lookup.
    expect([...glyphs].map((glyph) => glyph.getAttribute("data-glyph"))).toEqual(["draft"]);
    expect(planRow("atlas-03").querySelectorAll("svg")).toHaveLength(1);
  });
});

describe("the pressure record on a plan row", () => {
  const withDate = (slug: string, state: string, date: string | null) => ({
    ...plan(slug, state),
    pressure_date: date,
  });

  it("reads the test date, or that there is none, after the plan's short name", () => {
    const rows = [
      withDate("atlas-01", "written", "2026-10-20"),
      withDate("atlas-02", "written", null),
    ];
    renderSpine("/f/atlas/engineering", atlas({ plan_rows: rows }));

    expect(planRow("atlas-01").textContent).toBe("01 Pressure-tested 2026-10-20");
    expect(planRow("atlas-02").textContent).toBe("02 Not pressure-tested");
  });

  it("reads the record on a running and a done plan, and nothing on a retired one", () => {
    const rows = [
      withDate("atlas-01", "running", null),
      withDate("atlas-02", "done", "2026-10-01"),
      withDate("atlas-03", "retired", "2026-09-01"),
    ];
    renderSpine("/f/atlas/engineering", atlas({ plan_rows: rows }));

    expect(planRow("atlas-01").textContent).toContain("Not pressure-tested");
    expect(planRow("atlas-02").textContent).toContain("Pressure-tested 2026-10-01");
    expect(planRow("atlas-03").textContent).not.toMatch(/pressure/i);
  });

  it("has no Pressure test group", () => {
    renderSpine("/f/atlas/engineering");

    expect(within(spine()).queryByRole("link", { name: /^Pressure test\b/ })).toBeNull();
  });

  it("leads a row with one hidden glyph whose word is read, not seen", () => {
    const rows = [
      withDate("atlas-01", "done", "2026-10-20"),
      withDate("atlas-02", "retired", null),
    ];
    renderSpine("/f/atlas/engineering", atlas({ plan_rows: rows }));

    // The glyph's tooltip is its word, the name reads it after the short slug, and the slug is the
    // row's only visible text; the pressure record has no glyph (the plan page draws the flask).
    const done = link("01 Done Pressure-tested 2026-10-20");
    const glyphs = [...done.querySelectorAll(".plan-spine__glyph")];
    expect(
      glyphs.map((glyph) => [
        glyph.getAttribute("aria-hidden"),
        glyph.getAttribute("title"),
        glyph.getAttribute("data-glyph"),
        glyph.querySelector("svg") !== null,
      ]),
    ).toEqual([["true", "Done", "done", true]]);
    expect(done.querySelectorAll("svg")).toHaveLength(1);
    expect(done.querySelector(".plan-spine__badge")).toBeNull();
    expect(visibleText(done)).toBe("01");
    expect(done.querySelector(".plan-spine__glyph[aria-label]")).toBeNull();

    const retired = link("02 Retired");
    const glyph = retired.querySelector(".plan-spine__glyph");
    expect(glyph?.getAttribute("title")).toBe("Retired");
    expect(glyph?.getAttribute("data-glyph")).toBe("retired");
    expect(visibleText(retired)).toBe("02");
  });

  it("keeps a section's note as a visible word beside a wordless dashed glyph", () => {
    renderSpine("/f/atlas/product");

    const missing = within(spine()).getByText("Section 02").closest("li")!;
    const glyph = missing.querySelector(".plan-spine__glyph");
    expect(glyph?.getAttribute("data-glyph")).toBe("not-written");
    expect(glyph?.hasAttribute("title")).toBe(false);
    expect(visibleText(missing)).toBe("02 Section 02 not written");
  });

  it("lays a plan row out on one line, the slug giving way before the glyph", () => {
    const css = spineSheet.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, " ");
    expect(css).toContain(".plan-spine__glyph { display: inline-flex; flex: none;");
    expect(css).toContain(".plan-spine__glyph > svg { width: 14px; height: 14px; }");
    // No rule breaks the name onto its own line or clips a date: the row has no second line.
    expect(css).not.toContain(":has(> .plan-spine__badge)");
    expect(css).not.toContain("@container spine-row");
    expect(css).not.toContain(".plan-spine__badge-date");
  });

  it("draws no badge on any row and reads the record on plan rows only", () => {
    renderSpine("/f/atlas/engineering");

    const anchors = within(spine()).getAllByRole("link");
    expect(anchors.some((anchor) => anchor.querySelector(".plan-spine__badge") !== null)).toBe(
      false,
    );
    const reading = anchors.filter((anchor) => /pressure/i.test(anchor.textContent ?? ""));
    expect(reading.map((anchor) => anchor.textContent?.split(" ")[0])).toEqual(["01", "02"]);
  });
});
