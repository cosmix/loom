import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { DaemonLine, TokenLine } from "@/components/header-lines";
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
  it("shows the plan total and cache hit without a sparkline", () => {
    renderTokenLine();

    const rail = screen.getByRole("region", { name: "tokens" });
    for (const text of ["2.23M", "93%"]) expect(rail.textContent).toContain(text);
    expect(rail.textContent).not.toContain("43.2k");
    expect(within(rail).queryAllByRole("img")).toHaveLength(0);
  });

  it("keeps the two cells in one row with no wrap rules", () => {
    renderTokenLine();

    const rail = screen.getByRole("region", { name: "tokens" });
    const cells = within(rail).getAllByRole("listitem");
    expect(cells).toHaveLength(2);
    for (const cell of cells) expect(cell.className).not.toContain("max-sm:");
  });
});

describe("DaemonLine busy cue", () => {
  function renderDaemonLine(elapsedSecs: number) {
    const raw = structuredClone({
      ...fixture,
      busy: {
        op: "merge-verification",
        what: "verifying the merge of stage-a",
        stage_id: "stage-a",
        started_at: new Date(Date.now() - elapsedSecs * 1000).toISOString(),
        ceiling_secs: 1200,
        global_ceiling_secs: 1800,
      },
    });
    const snapshot = snapshotSchema.parse(raw);
    return render(<DaemonLine snapshot={snapshot} />, { wrapper: Provider });
  }

  it("adds a text cue when the operation is over its ceiling", () => {
    const { container } = renderDaemonLine(1300);

    const cue = container.querySelector(".sr-only");
    expect(cue?.textContent).toBe(", longer than expected");
    expect(container.firstElementChild?.getAttribute("title")).toContain("longer than expected");
  });

  it("adds no cue under the ceiling", () => {
    const { container } = renderDaemonLine(90);

    expect(container.querySelector(".sr-only")).toBeNull();
    expect(container.firstElementChild?.getAttribute("title")).toBeNull();
  });
});
