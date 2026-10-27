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
    fireEvent.click(screen.getByRole("button", { name: /Step 2 of 2 · Product spec/ }));
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
  });

  it("renders nothing before the first checklist", () => {
    const { container } = render(<StepTracker checklist={null} narrow={false} />);
    expect(container.innerHTML).toBe("");
  });
});
