import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { TokenLine } from "@/components/header-lines";
import { applySnapshot } from "@/state/apply";

function renderTokenLine(mutate?: (raw: typeof fixture) => void) {
  const raw = structuredClone(fixture);
  mutate?.(raw);
  const store = createStore();
  applySnapshot(store, snapshotSchema.parse(raw));
  return render(
    <Provider store={store}>
      <TokenLine />
    </Provider>,
  );
}

afterEach(cleanup);

describe("TokenLine", () => {
  it("left-pads a short plan series to the sparkline's 30 points", () => {
    renderTokenLine((raw) => {
      raw.status.tokens.plan.series = [9, 9];
    });

    const chart = screen.getByRole("img", { name: "plan fresh tokens, last 15 min" });
    const d = chart.querySelectorAll("path")[1]?.getAttribute("d") ?? "";
    expect((d.match(/[ML]/g) ?? []).length).toBe(30);
  });

  it("wraps the phone rail into two rows ruled apart, with no left rule on a row's first cell", () => {
    renderTokenLine();

    const rail = screen.getByRole("region", { name: "tokens" });
    const cells = within(rail).getAllByRole("listitem");
    expect(cells).toHaveLength(4);
    // The variants pick the cells: `odd` is the first of each row, `n+3` the second row.
    for (const cell of cells) {
      expect(cell.className).toContain("max-sm:odd:border-l-0");
      expect(cell.className).toContain("max-sm:nth-[n+3]:border-t");
    }
  });
});
