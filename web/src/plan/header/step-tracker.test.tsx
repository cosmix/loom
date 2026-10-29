import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { StepTracker } from "@/plan/header/step-tracker";

afterEach(cleanup);

const checklist = checklistViewSchema.parse({
  ...checklistFixture,
  steps: [
    ...checklistFixture.steps,
    { number: 3, name: "Metaplan", mark: "active" },
    { number: 4, name: "Plans", mark: "optional" },
    { number: 5, name: "Plan review", mark: "optional" },
    { number: 6, name: "Run review", mark: "optional" },
    { number: 7, name: "Execute", mark: "optional" },
  ],
});

describe("StepTracker cells", () => {
  it("marks the current step with aria-current and spells each mark for screen readers", () => {
    render(<StepTracker checklist={checklist} narrow={false} />);
    const list = screen.getByRole("list", { name: "Steps" });
    const items = within(list).getAllByRole("listitem");
    expect(list.tagName).toBe("OL");
    expect(items).toHaveLength(7);
    expect(items[0]!.getAttribute("aria-current")).toBeNull();
    expect(items[1]!.getAttribute("aria-current")).toBe("step");
    expect(screen.getAllByRole("listitem", { current: "step" })).toHaveLength(1);
    expect(items[0]!.querySelector(".sr-only")?.textContent).toBe(", done");
    expect(items[1]!.querySelector(".sr-only")?.textContent).toBe(", has gaps");
    expect(items[2]!.querySelector(".sr-only")?.textContent).toBe(", in progress");
    expect(items[3]!.querySelector(".sr-only")?.textContent).toBe(", optional");
  });

  it.each([
    [1, "Vision"],
    [2, "Product spec"],
    [3, "Metaplan"],
    [4, "Plans"],
    [5, "Plan review"],
    [6, "Run review"],
    [7, "Execute"],
  ] as const)("shows step %i and its name %s in a counter cell", (number, name) => {
    render(<StepTracker checklist={checklist} narrow={false} />);

    const label = screen.getByText(name);
    expect(label.classList.contains("plan-step-name")).toBe(true);
    expect(label.closest(".sr-only, .hidden, [hidden], [aria-hidden='true']")).toBeNull();
    const count = within(label.closest("li")!).getByText(String(number));
    expect(count.classList.contains("tabular-nums")).toBe(true);
  });
});

describe("StepTracker glyphs", () => {
  it.each([
    ["Vision", "lucide-circle-check", "text-(--tone-completed)"],
    ["Product spec", "lucide-book-open", "text-(--tone-warning)"],
    ["Metaplan", "lucide-map", "text-(--tone-executing)"],
    ["Plans", "lucide-list-checks", "text-(--tone-dimmed)"],
    ["Plan review", "lucide-gauge", "text-(--tone-dimmed)"],
    ["Run review", "lucide-scan-eye", "text-(--tone-dimmed)"],
    ["Execute", "lucide-play", "text-(--tone-dimmed)"],
  ])("renders %s with its glyph %s and tone %s", (name, glyphClass, tone) => {
    render(<StepTracker checklist={checklist} narrow={false} />);

    const glyph = screen.getByText(name).closest("li")!.querySelector("svg")!;
    expect(glyph.classList.contains(glyphClass)).toBe(true);
    expect(glyph.classList.contains(tone)).toBe(true);
    expect(glyph.getAttribute("aria-hidden")).toBe("true");
  });
});

describe("StepTracker wide layout", () => {
  it("tints the current cell and underlines it with a two-pixel primary accent", () => {
    render(<StepTracker checklist={checklist} narrow={false} />);

    const current = screen.getByRole("listitem", { current: "step" });
    expect(current.classList.contains("bg-muted")).toBe(true);
    expect(current.classList.contains("border-b-2")).toBe(true);
    expect(current.classList.contains("border-b-primary")).toBe(true);
    const done = screen.getByText("Vision").closest("li")!;
    expect(done.classList.contains("border-b-primary")).toBe(false);
  });

  it("contains the wide rail in a horizontal scroll container", () => {
    render(<StepTracker checklist={checklist} narrow={false} />);

    const list = screen.getByRole("list", { name: "Steps" });
    expect(list.classList.contains("plan-step-rail-vertical")).toBe(false);
    expect(list.parentElement!.classList.contains("min-w-0")).toBe(true);
    expect(list.parentElement!.classList.contains("max-w-full")).toBe(true);
    expect(list.parentElement!.classList.contains("overflow-x-auto")).toBe(true);
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
  it("collapses to a summary button whose popover lists the steps when narrow", () => {
    render(<StepTracker checklist={checklist} narrow />);
    expect(screen.queryByRole("list")).toBeNull();
    const trigger = screen.getByRole("button", { name: "Step 2 of 7 · Product spec" });
    // shrink-0 on Button's base class would stop the inner truncate from ever applying.
    expect(trigger.classList.contains("shrink")).toBe(true);
    expect(trigger.classList.contains("shrink-0")).toBe(false);
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: "Steps" });
    // The portal needs its own planner scope for the rail's CSS.
    expect(dialog.hasAttribute("data-loom-plan")).toBe(true);
    const list = within(dialog).getByRole("list", { name: "Steps" });
    expect(list.classList.contains("plan-step-rail-vertical")).toBe(true);
    expect(within(list).getAllByRole("listitem")).toHaveLength(7);
    expect(within(list).getByRole("listitem", { current: "step" }).textContent).toContain(
      "Product spec",
    );
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
