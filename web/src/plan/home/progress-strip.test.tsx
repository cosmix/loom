import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import type { StepView } from "@/plan/api/schema";
import { ProgressStrip } from "@/plan/home/progress-strip";

afterEach(cleanup);

const steps: StepView[] = [
  { number: 1, name: "Vision", mark: "done" },
  { number: 2, name: "Product spec", mark: "done" },
  { number: 3, name: "Metaplan", mark: "gap" },
  { number: 4, name: "Plans", mark: "active" },
  { number: 5, name: "Pressure", mark: "optional" },
  { number: 6, name: "Review", mark: "optional" },
  { number: 7, name: "Run", mark: "optional" },
];

function renderStrip(value: StepView[]) {
  return render(
    <Provider store={createStore()}>
      <MemoryRouter>
        <ProgressStrip steps={value} />
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
    expect(cells).toHaveLength(7);
    expect(cells.map((cell) => within(cell).getByText(/: /).textContent)).toEqual([
      "Vision: done",
      "Product spec: done",
      "Metaplan: gap",
      "Plans: active",
      "Pressure: optional",
      "Review: optional",
      "Run: optional",
    ]);
  });

  it("distinguishes the four marks with visible glyphs", () => {
    renderStrip(steps);

    const list = screen.getByRole("list", { name: "Progress" });
    expect(within(list).getAllByText("✓")).toHaveLength(2);
    expect(within(list).getByText("!")).toBeTruthy();
    expect(within(list).getByText("●")).toBeTruthy();
    expect(within(list).getAllByText("−")).toHaveLength(3);
  });

  it("escapes control characters in server step names", () => {
    renderStrip([{ number: 1, name: "Vision\u202E", mark: "done" }]);

    expect(screen.getByText("Vision⟨U+202E⟩: done")).toBeTruthy();
  });

  it("renders no cells for an empty step list", () => {
    renderStrip([]);

    expect(screen.queryByRole("listitem")).toBeNull();
  });
});
