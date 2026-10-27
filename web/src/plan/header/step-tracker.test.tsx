import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { StepTracker } from "@/plan/header/step-tracker";

afterEach(cleanup);

const checklist = checklistViewSchema.parse(checklistFixture);

describe("StepTracker", () => {
  it("marks the active step with aria-current and spells each mark for screen readers", () => {
    render(<StepTracker checklist={checklist} narrow={false} />);
    const items = screen.getAllByRole("listitem");
    expect(items[0]!.getAttribute("aria-current")).toBeNull();
    expect(items[1]!.getAttribute("aria-current")).toBe("step");
    expect(items[0]!.textContent).toContain("done");
    expect(items[1]!.textContent).toContain("has gaps");
  });

  it("keeps non-current step names in the accessibility tree below xl", () => {
    render(<StepTracker checklist={checklist} narrow={false} />);
    const name = screen.getByText("Vision").classList;
    expect(name.contains("sr-only")).toBe(true);
    expect(name.contains("xl:not-sr-only")).toBe(true);
    expect(name.contains("hidden")).toBe(false);
    expect(screen.getByText("Product spec").classList.contains("sr-only")).toBe(false);
  });

  it("marks no step current and says Ready to run when ready", () => {
    render(<StepTracker checklist={{ ...checklist, current: { kind: "ready" } }} narrow={false} />);
    expect(screen.queryByRole("listitem", { current: "step" })).toBeNull();
    expect(screen.getByText("Ready to run")).toBeTruthy();
  });

  it("says All plans done when every plan is done", () => {
    render(
      <StepTracker
        checklist={{ ...checklist, current: { kind: "ready_all_done" } }}
        narrow={false}
      />,
    );
    expect(screen.getByText("All plans done")).toBeTruthy();
  });

  it("collapses to a summary button whose popover lists the steps when narrow", () => {
    render(<StepTracker checklist={checklist} narrow />);
    expect(screen.queryByRole("list")).toBeNull();
    const trigger = screen.getByRole("button", { name: /Step 2 of 2 · Product spec/ });
    // shrink-0 on Button's base class would stop the inner truncate from ever applying.
    expect(trigger.classList.contains("shrink")).toBe(true);
    expect(trigger.classList.contains("shrink-0")).toBe(false);
    fireEvent.click(trigger);
    expect(screen.getByRole("dialog", { name: "Steps" })).toBeTruthy();
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
  });

  it("renders nothing before the first checklist", () => {
    const { container } = render(<StepTracker checklist={null} narrow={false} />);
    expect(container.innerHTML).toBe("");
  });
});
