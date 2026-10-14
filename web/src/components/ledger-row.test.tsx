import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { routes } from "@/router";
import { applySnapshot } from "@/state/apply";

function renderLedger(mutate?: (raw: typeof fixture) => void) {
  const raw = structuredClone(fixture);
  mutate?.(raw);
  const store = createStore();
  applySnapshot(store, snapshotSchema.parse(raw));
  const router = createMemoryRouter(routes, { initialEntries: ["/ledger"] });
  return render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

function tokensCell(stageId: string): Element {
  const row = screen
    .getAllByRole("row")
    .find((candidate) =>
      Array.from(candidate.querySelectorAll('[data-label="stage"] span')).some(
        (span) => span.textContent === stageId,
      ),
    );
  const cell = row?.querySelector('[data-label="tokens"]');
  if (!cell) throw new Error(`the tokens cell for ${stageId} is missing`);
  return cell;
}

/// Points on a sparkline's stroke path: one `M` or `L` command per value.
function strokePoints(chart: Element): number {
  const d = chart.querySelectorAll("path")[1]?.getAttribute("d") ?? "";
  return (d.match(/[ML]/g) ?? []).length;
}

afterEach(cleanup);

describe("ledger tokens cell", () => {
  it("shows an executing stage's rate with a sparkline", () => {
    renderLedger();

    expect(tokensCell("server").textContent).toContain("43.2k/m");
    expect(tokensCell("server").querySelector("svg")).not.toBeNull();
  });

  it("left-pads a short series to the sparkline's 20 points", () => {
    renderLedger((raw) => {
      const stage = raw.status.tokens.stages[0];
      if (!stage) throw new Error("fixture server stage is missing");
      stage.series = [4, 8];
    });

    const chart = tokensCell("server").querySelector("svg");
    if (!chart) throw new Error("the executing stage has no sparkline");
    expect(strokePoints(chart)).toBe(20);
  });

  it("shows the total, dimmed, for a stage that is neither executing nor completed", () => {
    renderLedger((raw) => {
      const stage = raw.status.tokens.stages[0];
      if (!stage) throw new Error("fixture server stage is missing");
      stage.stage_id = "docs";
    });

    const cell = tokensCell("docs");
    expect(cell.textContent).toBe("2.23M total");
    expect(cell.querySelector(".text-muted-foreground")).not.toBeNull();
  });

  it("renders nothing for a stage without a flow, so the card layout hides the cell", () => {
    renderLedger();

    const cell = tokensCell("design");
    expect(cell.textContent).toBe("");
    expect(cell.matches(":empty")).toBe(true);
  });
});
