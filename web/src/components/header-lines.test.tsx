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
  it("shows the numbers without a sparkline", () => {
    renderTokenLine();

    const rail = screen.getByRole("region", { name: "tokens" });
    for (const text of ["43.2k", "2.23M", "93%"]) expect(rail.textContent).toContain(text);
    expect(within(rail).queryAllByRole("img")).toHaveLength(0);
  });

  it("keeps the three cells in one row with no wrap rules", () => {
    renderTokenLine();

    const rail = screen.getByRole("region", { name: "tokens" });
    const cells = within(rail).getAllByRole("listitem");
    expect(cells).toHaveLength(3);
    for (const cell of cells) expect(cell.className).not.toContain("max-sm:");
  });
});
