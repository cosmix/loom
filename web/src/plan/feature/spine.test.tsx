import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import { checklistViewSchema, serverInfoSchema, type ChecklistView } from "@/plan/api/schema";
import { isSourcePath, Spine } from "@/plan/feature/spine";
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
const link = (name: string | RegExp) => within(spine()).getByRole("link", { name });
const current = () => within(spine()).getAllByRole("link", { current: "page" });

describe("rows", () => {
  it("lists the five groups in workflow order, each a link to its page", () => {
    renderSpine("/f/atlas/product");

    const heads = within(spine())
      .getAllByRole("link")
      .filter((anchor) => anchor.classList.contains("plan-spine__head"));
    expect(
      heads.map((anchor) => [anchor.textContent?.split(",")[0], anchor.getAttribute("href")]),
    ).toEqual([
      ["Vision done", "/f/atlas/vision"],
      ["Product spec 0/1", "/f/atlas/product"],
      ["Metaplan done", "/f/atlas/metaplan"],
      ["Plans 1/1", "/f/atlas/engineering"],
      ["Run optional", "/f/atlas/run"],
    ]);
  });

  it("draws each mark as a glyph and a word, from the server's step", () => {
    renderSpine("/f/atlas/product");

    const vision = link(/\bVision\b/);
    expect(vision.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
    expect(vision.textContent).toContain("done");
    expect(link(/\bProduct spec\b/).textContent).toContain("0/1");
    expect(link(/\bProduct spec\b/).textContent).toContain("has gaps");
  });

  it("reads a shown unit once, with no hidden word repeating it", () => {
    const steps = checklistFixture.checklists[0]!.steps.map((step) =>
      step.number === 4 ? { ...step, label: "Done" } : step,
    );
    renderSpine("/f/atlas/product", atlas({ steps }));

    // Visible "1/1 done" already names the state; a bare count ("0/1") keeps its hidden word.
    expect(link(/\bPlans\b/).textContent).toBe("Plans 1/1 done");
    expect(link(/\bProduct spec\b/).textContent).toBe("Product spec 0/1, has gaps");
  });

  it("hides no group for a feature with no product section", () => {
    renderSpine("/f/atlas/vision", atlas({ sections: [], steps: [] }));

    expect(link(/\bProduct spec\b/).textContent).toContain("not started");
    for (const name of [/\bVision\b/, /\bMetaplan\b/, /\bPlans\b/, /\bRun\b/]) {
      expect(link(name)).toBeTruthy();
    }
  });

  it("names a feature's plans by slug and states each plan's state", () => {
    renderSpine("/f/atlas/engineering");

    const first = link(/atlas-01/);
    expect(first.getAttribute("href")).toBe("/f/atlas/engineering/atlas-01");
    expect(first.textContent).toContain("done");
    expect(link(/atlas-02/).getAttribute("href")).toBe("/f/atlas/engineering/atlas-02");
    expect(link(/atlas-02/).textContent).toContain("running");
  });

  it("links written sections and lists a missing one as dim text with no link", () => {
    renderSpine("/f/atlas/product");

    const written = link(/The Run loop/);
    expect(written.getAttribute("href")).toBe("/f/atlas/product/section_01");
    expect(within(written).getByText("Run").tagName).toBe("CODE");
    expect(within(spine()).queryByRole("link", { name: /Section 02/ })).toBeNull();
    const missing = within(spine()).getByText("Section 02").closest("li")!;
    expect(missing.textContent).toContain("not written");
    expect(link("Overview").getAttribute("href")).toBe("/f/atlas/product");
  });

  it("lists the metaplans as documents only when there is more than one", () => {
    const one = [{ path: "doc/plans/atlas/index.md", title: "Roadmap" }];
    const { unmount } = renderSpine("/f/atlas/metaplan", atlas({ metaplans: one }));
    expect(within(spine()).queryByRole("link", { name: /Roadmap/ })).toBeNull();
    unmount();

    renderSpine(
      "/f/atlas/metaplan",
      atlas({ metaplans: [...one, { path: "doc/plans/atlas/round-2.md", title: "Round two" }] }),
    );
    expect(link(/Round two/).getAttribute("href")).toBe("/f/atlas/metaplan/round-2.md");
  });

  it("sets server names through Untrusted and carries no step class", () => {
    renderSpine("/f/atlas/engineering", atlas({ plan_rows: [plan("de‮mo", "done")] }));

    expect(link(/de⟨U\+202E⟩mo/)).toBeTruthy();
    expect(spine().querySelector('[class*="plan-step"]')).toBeNull();
  });
});

describe("hostile names and marks", () => {
  it("renders for a feature named like an Object.prototype member", () => {
    renderSpine("/f/constructor/product", atlas({ feature: "constructor" }));

    expect(link(/\bPlans\b/)).toBeTruthy();
    fireEvent.click(within(spine()).getByRole("button", { name: "Expand Plans" }));
    expect(link(/atlas-01/)).toBeTruthy();
  });

  it("draws a plan whose state names an Object.prototype member with no icon", () => {
    renderSpine("/f/atlas/engineering", atlas({ plan_rows: [plan("atlas-03", "constructor")] }));

    const row = link(/atlas-03/);
    expect(row.textContent).toContain("constructor");
    // The pressure badge draws its own glyph; the state draws none.
    expect(row.querySelector(".plan-status svg")).toBeNull();
  });
});

describe("the pressure record on a plan row", () => {
  const withDate = (slug: string, state: string, date: string | null) => ({
    ...plan(slug, state),
    pressure_date: date,
  });

  it("reads the test date, or that there is none, after the plan's state", () => {
    const rows = [
      withDate("atlas-01", "written", "2026-10-20"),
      withDate("atlas-02", "written", null),
    ];
    renderSpine("/f/atlas/engineering", atlas({ plan_rows: rows }));

    const tested = link(/atlas-01/);
    expect(tested.textContent).toBe("atlas-01 written Pressure-tested 2026-10-20");
    expect(tested.querySelector('[data-tested="true"]')).not.toBeNull();
    expect(link(/atlas-02/).querySelector('[data-tested="false"]')?.textContent).toBe(
      "Not pressure-tested",
    );
  });

  it("shows the record on a running and a done plan, and nothing on a retired one", () => {
    const rows = [
      withDate("atlas-01", "running", null),
      withDate("atlas-02", "done", "2026-10-01"),
      withDate("atlas-03", "retired", "2026-09-01"),
    ];
    renderSpine("/f/atlas/engineering", atlas({ plan_rows: rows }));

    expect(link(/atlas-01/).textContent).toContain("Not pressure-tested");
    expect(link(/atlas-02/).textContent).toContain("Pressure-tested 2026-10-01");
    expect(link(/atlas-03/).querySelector(".plan-spine__badge")).toBeNull();
  });

  it("has no Pressure test group", () => {
    renderSpine("/f/atlas/engineering");

    expect(within(spine()).queryByRole("link", { name: /^Pressure test\b/ })).toBeNull();
  });
});

describe("the current row", () => {
  it("marks the overview with aria-current and no other row", () => {
    renderSpine("/f/atlas/product");

    expect(current().map((anchor) => anchor.textContent)).toEqual(["Overview"]);
  });

  it("marks a section, and a row with no children by its own link", () => {
    renderSpine("/f/atlas/product/section_01");
    expect(current().map((anchor) => anchor.getAttribute("href"))).toEqual([
      "/f/atlas/product/section_01",
    ]);
    cleanup();

    renderSpine("/f/atlas/vision");
    expect(current().map((anchor) => anchor.getAttribute("href"))).toEqual(["/f/atlas/vision"]);
  });
});

describe("groups", () => {
  it("expands the route's group and collapses the rest", () => {
    renderSpine("/f/atlas/product");

    expect(within(spine()).queryByRole("link", { name: /atlas-01/ })).toBeNull();
    expect(within(spine()).getByRole("link", { name: "Overview" })).toBeTruthy();
    expect(within(spine()).queryByRole("button", { name: "Collapse Product spec" })).toBeNull();
  });

  it("expands another group on request and remembers it for this feature", () => {
    const first = renderSpine("/f/atlas/product");
    fireEvent.click(within(spine()).getByRole("button", { name: "Expand Plans" }));
    expect(link(/atlas-01/)).toBeTruthy();
    first.unmount();

    renderSpine("/f/atlas/product");
    expect(link(/atlas-01/)).toBeTruthy();
    fireEvent.click(within(spine()).getByRole("button", { name: "Collapse Plans" }));
    expect(within(spine()).queryByRole("link", { name: /atlas-01/ })).toBeNull();
  });
});

describe("the Documents control", () => {
  it("names the current row and opens and closes the list", () => {
    renderSpine("/f/atlas/product/section_01");

    const control = within(spine()).getByRole("button", { name: /Documents/ });
    expect(control.textContent).toContain("The Run loop");
    expect(control.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(control);
    expect(control.getAttribute("aria-expanded")).toBe("true");
    expect(spine().hasAttribute("data-open")).toBe(true);
  });

  it("closes again when the route changes", () => {
    const { router } = renderSpine("/f/atlas/product");
    const control = within(spine()).getByRole("button", { name: /Documents/ });
    fireEvent.click(control);

    fireEvent.click(link(/\bVision\b/));

    expect(router.state.location.pathname).toBe("/f/atlas/vision");
    expect(control.getAttribute("aria-expanded")).toBe("false");
    expect(control.textContent).toContain("Vision");
  });
});

describe("the next step", () => {
  it("heads the list of groups", () => {
    renderSpine("/f/atlas/vision");

    const body = spine().querySelector(".plan-spine__body");
    expect(body?.firstElementChild?.className).toBe("plan-spine-next");
    expect(body?.lastElementChild?.tagName).toBe("OL");
  });
});

describe("a document's source", () => {
  it.each([
    ["/f/atlas/source/product/atlas/index.md", true],
    ["/f/atlas/source/plans/atlas/PLAN-atlas-01.md", true],
    ["/f/atlas/vision", false],
    ["/f/source/vision", false],
    ["/files/product/atlas/index.md", false],
  ])("reads %s as a source route: %s", (pathname, expected) => {
    expect(isSourcePath(pathname)).toBe(expected);
  });

  it("marks the spine on a source route, and only there", () => {
    renderSpine("/f/atlas/source/product/atlas/index.md");
    expect(spine().getAttribute("data-view")).toBe("source");
    cleanup();

    renderSpine("/f/atlas/vision");
    expect(spine().hasAttribute("data-view")).toBe(false);
  });

  it("draws Source under the current row, or the group's head when no row is current", () => {
    const css = spineSheet.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, " ");
    expect(css).toContain(
      '.plan-spine[data-view="source"] .plan-spine__link[aria-current="page"]::after',
    );
    expect(css).toContain('.plan-spine__group[data-here]:not(:has([aria-current="page"]))');
    expect(css).toContain('content: "Source";');
  });

  it.each([
    ["/f/atlas/source/product/atlas/index.md", "Vision", "/f/atlas/vision"],
    [
      "/f/atlas/source/product/atlas/section_01.md",
      "01 The Run loop",
      "/f/atlas/product/section_01",
    ],
    [
      "/f/atlas/source/plans/atlas/PLAN-atlas-02.md",
      "atlas-02 running",
      "/f/atlas/engineering/atlas-02",
    ],
  ])("marks the row of the document shown at %s as the one current link", (path, name, href) => {
    renderSpine(path);
    const [only, ...rest] = current();
    expect(rest).toEqual([]);
    expect(only!.getAttribute("href")).toBe(href);
    expect(only!.textContent?.startsWith(name.split(" ")[0]!)).toBe(true);
  });

  it("marks the group's head for a lone metaplan, and no row for a file with no row", () => {
    renderSpine(
      "/f/atlas/source/plans/atlas/index.md",
      atlas({ metaplans: [{ path: "doc/plans/atlas/index.md", title: "Roadmap" }] }),
    );
    expect(current().map((anchor) => anchor.getAttribute("href"))).toEqual(["/f/atlas/metaplan"]);
    cleanup();

    renderSpine("/f/atlas/source/product/atlas/notes.md");
    expect(within(spine()).queryAllByRole("link", { current: "page" })).toEqual([]);
  });

  it("marks the group's head on the page of a lone metaplan that is not index.md", () => {
    const lone = atlas({ metaplans: [{ path: "doc/plans/atlas/round-1.md", title: "Round one" }] });
    renderSpine("/f/atlas/metaplan/round-1.md", lone);
    expect(current().map((anchor) => anchor.getAttribute("href"))).toEqual(["/f/atlas/metaplan"]);
  });
});
