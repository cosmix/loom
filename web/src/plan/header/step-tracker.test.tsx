import { cleanup, fireEvent, render as rtlRender, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactElement } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import runStripFixture from "@/plan/api/fixtures/run-strip.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { runStripSchema } from "@/plan/api/schema-run";
import { StepTracker } from "@/plan/header/step-tracker";
import { runStripAtom } from "@/plan/run/use-run";

afterEach(cleanup);

function render(ui: ReactElement, store = createStore()) {
  return rtlRender(
    <Provider store={store}>
      <MemoryRouter initialEntries={["/f/demo"]}>{ui}</MemoryRouter>
    </Provider>,
  );
}

const checklist = checklistViewSchema.parse({
  ...checklistFixture.checklists[0]!,
  steps: [
    ...checklistFixture.checklists[0]!.steps.slice(0, 2).map((step) =>
      step.number === 2 ? { ...step, count: "3/5" } : step,
    ),
    { number: 3, name: "Metaplan", mark: "active", count: null, label: "Metaplan" },
    { number: 4, name: "Plans", mark: "optional", count: null, label: "Plans" },
    { number: 5, name: "Plan review", mark: "optional", count: null, label: "Plan review" },
    { number: 6, name: "Execute", mark: "optional", count: null, label: "Execute" },
  ],
});

describe("StepTracker cells", () => {
  it("marks the current step with aria-current and spells each mark for screen readers", () => {
    render(<StepTracker checklist={checklist} narrow={false} />);
    const list = screen.getByRole("list", { name: "Steps" });
    const items = within(list).getAllByRole("listitem");
    expect(list.tagName).toBe("OL");
    expect(items).toHaveLength(6);
    expect(items[0]!.getAttribute("aria-current")).toBeNull();
    expect(items[1]!.getAttribute("aria-current")).toBe("step");
    expect(screen.getAllByRole("listitem", { current: "step" })).toHaveLength(1);
    expect(items[0]!.querySelector(".sr-only")?.textContent).toBe(", done");
    expect(items[1]!.querySelector(".sr-only")?.textContent).toBe(", has gaps");
    expect(items[2]!.querySelector(".sr-only")?.textContent).toBe(", in progress");
    expect(items[3]!.querySelector(".sr-only")?.textContent).toBe(", optional");
  });

  it.each([
    [1, "Vision", "1"],
    [2, "Product spec", "3/5"],
    [3, "Metaplan", "—"],
    [4, "Plans", "—"],
    [5, "Plan review", "—"],
    [6, "Execute", "—"],
  ] as const)("shows step %i as its server count over its label %s", (_number, name, count) => {
    render(<StepTracker checklist={checklist} narrow={false} />);

    const label = screen.getByText(name);
    expect(label.classList.contains("plan-step-name")).toBe(true);
    expect(label.closest(".sr-only, .hidden, [hidden], [aria-hidden='true']")).toBeNull();
    const counter = within(label.closest("li")!).getByText(count);
    expect(counter.classList.contains("tabular-nums")).toBe(true);
  });

  it("links each cell to its step on the feature route", () => {
    render(<StepTracker checklist={checklist} narrow={false} />);
    const links = within(screen.getByRole("list", { name: "Steps" })).getAllByRole("link");
    expect(links.map((link) => link.getAttribute("href"))).toEqual(
      [1, 2, 3, 4, 5, 6].map((number) => `/f/${checklist.feature}?step=${number}`),
    );
  });
});

describe("StepTracker glyphs", () => {
  it.each([
    ["Vision", "lucide-circle-check", "text-(--tone-completed)"],
    ["Product spec", "lucide-book-open", "text-foreground"],
    ["Metaplan", "lucide-map", "text-muted-foreground"],
    ["Plans", "lucide-list-checks", "text-muted-foreground"],
    ["Plan review", "lucide-gauge", "text-muted-foreground"],
    ["Execute", "lucide-play", "text-muted-foreground"],
  ])("renders %s with its glyph %s and tone %s", (name, glyphClass, tone) => {
    render(<StepTracker checklist={checklist} narrow={false} />);

    const glyph = screen.getByText(name).closest("li")!.querySelector("svg")!;
    expect(glyph.classList.contains(glyphClass)).toBe(true);
    expect(glyph.classList.contains(tone)).toBe(true);
    expect(glyph.getAttribute("aria-hidden")).toBe("true");
  });

  it("swaps the Run step's glyph for the running ring while a plan runs", () => {
    const store = createStore();
    store.set(runStripAtom, { ...runStripSchema.parse(runStripFixture), daemon_running: true });
    render(<StepTracker checklist={checklist} narrow={false} />, store);

    const execute = screen.getByText("Execute").closest("li")!;
    const ring = execute.querySelector("[data-kind='running']");
    expect(ring?.classList.contains("text-(--tone-executing)")).toBe(true);
    // The ring replaces the step glyph; its own play shape sits inside the ring.
    expect(execute.querySelector("svg.plan-step-glyph")).toBeNull();
    const other = screen.getByText("Plans").closest("li")!;
    expect(other.querySelector("[data-kind='running']")).toBeNull();
  });

  it("keeps the Run step's own glyph when no plan is running", () => {
    const store = createStore();
    store.set(runStripAtom, { ...runStripSchema.parse(runStripFixture), daemon_running: false });
    render(<StepTracker checklist={checklist} narrow={false} />, store);

    const execute = screen.getByText("Execute").closest("li")!;
    expect(execute.querySelector("svg.lucide-play.plan-step-glyph")).not.toBeNull();
    expect(execute.querySelector("[data-kind='running']")).toBeNull();
  });
});

describe("StepTracker wide layout", () => {
  it("gives only the current cell the primary wash and inset underline class", () => {
    render(<StepTracker checklist={checklist} narrow={false} />);

    const current = screen.getByRole("listitem", { current: "step" });
    expect(current.classList.contains("plan-step-cell-current")).toBe(true);
    expect(current.classList.contains("border-b-2")).toBe(false);
    const done = screen.getByText("Vision").closest("li")!;
    expect(done.classList.contains("plan-step-cell-current")).toBe(false);
  });

  it("wraps the rail in a shrinkable container", () => {
    render(<StepTracker checklist={checklist} narrow={false} />);

    const list = screen.getByRole("list", { name: "Steps" });
    expect(list.classList.contains("plan-step-rail")).toBe(true);
    expect(list.parentElement!.classList.contains("min-w-0")).toBe(true);
    expect(list.parentElement!.classList.contains("max-w-full")).toBe(true);
  });
});

describe("StepTracker ready states", () => {
  it.each([
    ["ready", "Ready to run"],
    ["ready_all_done", "All plans done"],
  ] as const)("marks no step current and shows %s as %s after the cells", (kind, label) => {
    render(<StepTracker checklist={{ ...checklist, current: { kind } }} narrow={false} />);

    expect(screen.queryByRole("listitem", { current: "step" })).toBeNull();
    const ready = screen.getByText(label);
    expect(ready.classList.contains("text-(--tone-completed)")).toBe(true);
    expect(screen.getByRole("list", { name: "Steps" }).nextElementSibling).toBe(ready);
  });
});

describe("StepTracker narrow layout and loading", () => {
  it("shows the current step label as visible text beside the chevron", () => {
    render(
      <StepTracker checklist={{ ...checklist, current: { kind: "step", number: 4 } }} narrow />,
    );

    const trigger = screen.getByRole("button", { name: "Step 4 of 6 · Plans" });
    const label = within(trigger).getByText("Step 4 of 6 · Plans");
    expect(label.closest(".sr-only, .hidden, [hidden], [aria-hidden='true']")).toBeNull();
    expect(label.nextElementSibling?.tagName.toLowerCase()).toBe("svg");
  });

  it("collapses to a summary button whose popover lists the steps when narrow", () => {
    render(<StepTracker checklist={checklist} narrow />);
    expect(screen.queryByRole("list")).toBeNull();
    const trigger = screen.getByRole("button", { name: "Step 2 of 6 · Product spec" });
    // The summary keeps its full text (flex: none, nowrap in header.css); the next-step button yields.
    expect(trigger.classList.contains("plan-rail-summary")).toBe(true);
    const label = within(trigger).getByText("Step 2 of 6 · Product spec");
    expect(label.classList.contains("truncate")).toBe(false);
    expect(label.closest(".sr-only, .hidden, [hidden], [aria-hidden='true']")).toBeNull();
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: "Steps" });
    // The portal needs its own planner scope for the rail's CSS.
    expect(dialog.hasAttribute("data-loom-plan")).toBe(true);
    const list = within(dialog).getByRole("list", { name: "Steps" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(6);
    expect(
      within(list)
        .getAllByRole("link")
        .map((link) => link.getAttribute("href")),
    ).toEqual([1, 2, 3, 4, 5, 6].map((number) => `/f/${checklist.feature}?step=${number}`));
    expect(within(list).getByRole("listitem", { current: "step" }).textContent).toContain(
      "Product spec",
    );
  });

  it("closes the popover when a step link is followed", () => {
    render(<StepTracker checklist={checklist} narrow />);
    fireEvent.click(screen.getByRole("button", { name: "Step 2 of 6 · Product spec" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Steps" })).getByText("Plans"));
    expect(screen.queryByRole("dialog", { name: "Steps" })).toBeNull();
  });

  it.each([
    ["ready", "Ready to run"],
    ["ready_all_done", "All plans done"],
  ] as const)("uses %s as the narrow summary %s", (kind, label) => {
    render(<StepTracker checklist={{ ...checklist, current: { kind } }} narrow />);

    expect(screen.getByRole("button", { name: label })).toBeTruthy();
    expect(screen.queryByRole("list")).toBeNull();
  });

  it.each([false, true])("renders nothing before the first checklist (narrow=%s)", (narrow) => {
    const { container } = render(<StepTracker checklist={null} narrow={narrow} />);
    expect(container.innerHTML).toBe("");
  });
});
