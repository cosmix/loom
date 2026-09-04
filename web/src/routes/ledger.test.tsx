import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeAll, describe, expect, it } from "vitest";

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

beforeAll(() => {
  // jsdom has no ResizeObserver; the shadcn ScrollArea asks for one on mount.
  if (!("ResizeObserver" in window)) {
    class ResizeObserverStub {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    Object.defineProperty(window, "ResizeObserver", { value: ResizeObserverStub });
  }
});

afterEach(cleanup);

describe("ledger route", () => {
  it("renders the fixture: plan name, every stage, attention labels, logo", () => {
    renderAt("/");

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
});

describe("stage route", () => {
  it("shows the server stage's last tool and model", () => {
    renderAt("/stages/server");

    expect(screen.getAllByText("Bash").length).toBeGreaterThan(0);
    expect(screen.getAllByText("opus").length).toBeGreaterThan(0);
  });
});
