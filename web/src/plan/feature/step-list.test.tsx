import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { StepList } from "@/plan/feature/step-list";

afterEach(() => {
  cleanup();
  Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
});

const checklist = checklistViewSchema.parse({
  feature: "demo",
  pitch: null,
  has_product: true,
  sections: [],
  plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
  untested: 0,
  metaplans: [],
  plan_rows: [],
  current: { kind: "step", number: 2 },
  steps: [
    { number: 1, name: "Vision", mark: "done", count: null, label: "Vision" },
    { number: 2, name: "Product spec", mark: "gap", count: null, label: "Product spec" },
    { number: 3, name: "Metaplan", mark: "active", count: null, label: "Metaplan" },
    { number: 4, name: "Plans", mark: "optional", count: null, label: "Plans" },
    {
      number: 6,
      name: "Server-defined step",
      mark: "optional",
      count: null,
      label: "Server-defined step",
    },
  ],
  primary: null,
  secondary: [],
  items: [],
});

function renderList(value: ChecklistView, selected = 2) {
  const store = createStore();
  return render(
    <Provider store={store}>
      <MemoryRouter>
        <StepList checklist={value} selected={selected} />
      </MemoryRouter>
    </Provider>,
  );
}

function stepLink(name: string): HTMLElement {
  const nav = screen.getByRole("navigation", { name: "Steps" });
  const link = within(nav)
    .getAllByRole("link")
    .find((candidate) => candidate.textContent?.startsWith(name));
  if (!link) throw new Error(`No step link starts with ${name}`);
  return link;
}

it("renders Conversation followed by every server step as links, with a line under each", () => {
  renderList(checklist);

  const nav = screen.getByRole("navigation", { name: "Steps" });
  expect(within(nav).getAllByRole("listitem")).toHaveLength(6);
  expect(
    within(nav)
      .getAllByRole("link")
      .map((link) => link.textContent),
  ).toEqual([
    "Conversation, done demo",
    "Vision, done 0 sections listed",
    "Product spec, has gaps 0 of 0 written",
    "Metaplan, in progress after the spec",
    "Plans, optional 0 plans",
    "Server-defined step, optional 0 done",
  ]);
});

it("links each server step to its query and Conversation to the feature", () => {
  renderList(checklist);

  expect(stepLink("Conversation").getAttribute("href")).toBe("/f/demo");
  expect(stepLink("Product spec").getAttribute("href")).toBe("/f/demo?step=2");
  expect(stepLink("Plans").getAttribute("href")).toBe("/f/demo?step=4");
});

it.each([
  ["Conversation", "done", "done"],
  ["Vision", "done", "done"],
  ["Product spec", "has gaps", "gap"],
  ["Metaplan", "in progress", "active"],
  ["Plans", "optional", "optional"],
] as const)("conveys the %s mark by glyph and screen-reader text", (name, label, stepMark) => {
  renderList(checklist);
  const link = stepLink(name);

  expect(link.getAttribute("data-step-mark")).toBe(stepMark);
  expect(within(link).getByText(`, ${label}`).className).toContain("sr-only");
  expect(link.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
});

it("puts aria-current on the selected step only and flags the server's current step apart", () => {
  renderList(checklist, 4);

  const nav = screen.getByRole("navigation", { name: "Steps" });
  const current = within(nav).getAllByRole("link", { current: "step" });
  expect(current.map((link) => link.textContent?.split(",")[0])).toEqual(["Plans"]);
  expect(stepLink("Product spec").getAttribute("aria-current")).toBeNull();
  expect(stepLink("Product spec").hasAttribute("data-current")).toBe(true);
  expect(stepLink("Plans").hasAttribute("data-current")).toBe(false);
  expect(stepLink("Conversation").getAttribute("aria-current")).toBeNull();
  expect(stepLink("Metaplan").getAttribute("aria-current")).toBeNull();
});

it("marks only the current step when it is the selected one", () => {
  renderList(checklist, 2);

  expect(screen.getAllByRole("link", { current: "step" })).toHaveLength(1);
  expect(stepLink("Product spec").getAttribute("aria-current")).toBe("step");
  expect(stepLink("Product spec").hasAttribute("data-current")).toBe(false);
});

it.each([
  ["ready", "Ready to run"],
  ["ready_all_done", "All plans done"],
] as const)("shows the %s label without a current step", (kind, label) => {
  renderList(checklistViewSchema.parse({ ...checklist, current: { kind } }), 6);

  expect(screen.getByRole("link", { current: "step" }).textContent).toContain(
    "Server-defined step",
  );
  expect(screen.getByText(label)).toBeTruthy();
});

it("renders server step names and the slug through Untrusted", () => {
  renderList(
    checklistViewSchema.parse({
      ...checklist,
      feature: "de\u202Emo",
      steps: [
        { number: 2, name: "Spec\u202Ename", mark: "gap", count: null, label: "Spec\u202Ename" },
      ],
    }),
  );

  expect(stepLink("Spec⟨U+202E⟩name")).toBeTruthy();
  expect(stepLink("Conversation").textContent).toContain("de⟨U+202E⟩mo");
});

it.each([
  [{ sections: 3, written: 1 }, "Vision", "3 sections listed"],
  [{ sections: 3, written: 1 }, "Product spec", "1 of 3 written"],
  [{ untested: 2 }, "Pressure", "2 untested"],
  [{ untested: 0 }, "Pressure", "all tested"],
  [{ running: 1 }, "Run", "1 running"],
] as const)("fills the line under a step from the counts %j", (counts, name, line) => {
  const sections = Array.from({ length: "sections" in counts ? counts.sections : 0 }, (_, i) => ({
    number: String(i + 1).padStart(2, "0"),
    name: `Section ${i + 1}`,
    written: "written" in counts && i < counts.written,
    open_questions: 0,
  }));
  renderList(
    checklistViewSchema.parse({
      ...checklist,
      sections,
      untested: "untested" in counts ? counts.untested : 0,
      plans: { ...checklist.plans, running: "running" in counts ? counts.running : 0 },
      steps: [
        { number: 1, name: "Vision", mark: "done", count: null, label: "Vision" },
        { number: 2, name: "Product spec", mark: "gap", count: null, label: "Product spec" },
        { number: 5, name: "Pressure", mark: "gap", count: null, label: "Pressure" },
        { number: 6, name: "Run", mark: "gap", count: null, label: "Run" },
      ],
    }),
  );

  expect(stepLink(name).textContent).toContain(line);
});

it("scrolls the selected step into view on mount and when the selection changes", () => {
  const reached: Element[] = [];
  const scrollIntoView = vi.fn(function (this: Element) {
    reached.push(this);
  });
  // jsdom has no scrollIntoView, so the component's guard is what lets the other tests render.
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: scrollIntoView,
  });
  const view = renderList(checklist, 2);

  expect(scrollIntoView).toHaveBeenCalledTimes(1);
  expect(scrollIntoView).toHaveBeenLastCalledWith({
    block: "nearest",
    inline: "nearest",
    behavior: "instant",
  });
  expect(reached[0]).toBe(stepLink("Product spec"));

  view.rerender(
    <Provider store={createStore()}>
      <MemoryRouter>
        <StepList checklist={checklist} selected={4} />
      </MemoryRouter>
    </Provider>,
  );

  expect(scrollIntoView).toHaveBeenCalledTimes(2);
  expect(reached[1]).toBe(stepLink("Plans"));
});
