import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { routes } from "@/router";
import { applySnapshot } from "@/state/apply";

const snapshot = snapshotSchema.parse(fixture);

function renderAt(path: string) {
  const store = createStore();
  applySnapshot(store, snapshot);
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  return render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

afterEach(cleanup);

describe("ledger route", () => {
  it("renders the fixture: plan name, every stage, attention labels, logo", () => {
    renderAt("/ledger");

    expect(screen.getByText("Web Dashboard Fixture")).toBeTruthy();
    for (const stage of snapshot.status.stages) {
      expect(screen.getAllByText(stage.id).length).toBeGreaterThan(0);
    }
    for (const label of ["ACCEPTANCE FAILED", "MERGE CONFLICT", "NEEDS REVIEW"]) {
      expect(screen.getAllByText(label).length).toBeGreaterThan(0);
    }
    expect(screen.getByRole("img", { name: "loom" })).toBeTruthy();
    expect(screen.getByText("orchestrator loop stalled 75s")).toBeTruthy();
    // The "server" stage is the only one with context tokens in the fixture
    // (312000 of an 800000 ceiling).
    expect(screen.getByText("39%")).toBeTruthy();
  });

  it("leaves the automatic merge conflict out of the header's attention count", () => {
    renderAt("/ledger");

    const rail = screen.getByRole("region", { name: "stages" });
    const cell = within(rail).getByText("attention").closest("li");
    if (!cell) throw new Error("the attention cell is missing");
    expect(snapshot.attention).toHaveLength(5);
    expect(within(cell).getByText("3")).toBeTruthy();
  });
});

describe("stage route", () => {
  it("shows the server stage's last tool and model", () => {
    renderAt("/stages/server");

    expect(screen.getAllByText("Bash").length).toBeGreaterThan(0);
    expect(screen.getAllByText("opus").length).toBeGreaterThan(0);
  });

  it("tells the operator what to do for a blocked stage and numbers its criteria", () => {
    renderAt("/stages/local-runtime");

    expect(screen.getByText("what to do")).toBeTruthy();
    expect(screen.getByText(/Or allow AF_UNIX for this stage\./)).toBeTruthy();
    expect(screen.getByRole("button", { name: "copy loom stage skip local-runtime" })).toBeTruthy();
    const criteria = within(screen.getByRole("region", { name: "acceptance criteria" }));
    expect(criteria.getAllByRole("listitem")).toHaveLength(5);
  });
});
