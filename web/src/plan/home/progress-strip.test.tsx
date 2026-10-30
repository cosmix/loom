import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import type { StepView } from "@/plan/api/schema";
import { ProgressStrip } from "@/plan/home/progress-strip";

afterEach(cleanup);

const steps: StepView[] = [
  { number: 1, name: "Vision", mark: "done", count: null, label: "Vision" },
  { number: 2, name: "Product spec", mark: "done", count: null, label: "Product spec" },
  { number: 3, name: "Metaplan", mark: "gap", count: null, label: "Metaplan" },
  { number: 4, name: "Plans", mark: "active", count: null, label: "Plans" },
  { number: 5, name: "Pressure", mark: "optional", count: null, label: "Pressure" },
  { number: 6, name: "Run", mark: "optional", count: null, label: "Run" },
];

function renderStrip(value: StepView[], current: number | null = 4, running = false) {
  return render(
    <Provider store={createStore()}>
      <MemoryRouter>
        <ProgressStrip steps={value} current={current} running={running} />
      </MemoryRouter>
    </Provider>,
  );
}

describe("ProgressStrip", () => {
  it("renders one labelled cell per step in order with its mark", () => {
    renderStrip(steps);

    const list = screen.getByRole("list", { name: "Progress" });
    const cells = within(list).getAllByRole("listitem");
    expect(list.tagName).toBe("OL");
    expect(cells).toHaveLength(6);
    expect(cells.map((cell) => within(cell).getByText(/: /).textContent)).toEqual([
      "Vision: done",
      "Product spec: done",
      "Metaplan: gap",
      "Plans: active",
      "Pressure: optional",
      "Run: optional",
    ]);
  });

  it("colours done steps, the current step and a running Run step; the rest stay idle", () => {
    renderStrip(steps, 4, true);

    const cells = within(screen.getByRole("list", { name: "Progress" })).getAllByRole("listitem");
    expect(cells.map((cell) => cell.getAttribute("data-cell"))).toEqual([
      "done",
      "done",
      "idle",
      "current",
      "idle",
      "running",
    ]);
    expect(cells[5].textContent).toBe("Run: running");
  });

  it("marks no step current when none is, and keeps Run idle while nothing runs", () => {
    renderStrip(steps, null, false);

    const cells = within(screen.getByRole("list", { name: "Progress" })).getAllByRole("listitem");
    expect(cells.map((cell) => cell.getAttribute("data-cell"))).toEqual([
      "done",
      "done",
      "idle",
      "idle",
      "idle",
      "idle",
    ]);
  });

  it("escapes control characters in server step names", () => {
    renderStrip([
      { number: 1, name: "Vision\u202E", mark: "done", count: null, label: "Vision\u202E" },
    ]);

    expect(screen.getByText("Vision⟨U+202E⟩: done")).toBeTruthy();
  });

  it("renders no cells for an empty step list", () => {
    renderStrip([]);

    expect(screen.queryByRole("listitem")).toBeNull();
  });
});
