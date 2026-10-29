import { cleanup, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/run-strip.json";
import { runStripSchema } from "@/plan/api/schema-run";
import { RunStrip } from "./run-strip";
import { runStripAtom } from "./use-run";

vi.mock("./dashboard-button", () => ({
  DashboardButton: () => <a href="/dashboard">Open dashboard</a>,
}));
afterEach(cleanup);

it("shows the run status and dashboard", () => {
  const store = createStore();
  store.set(runStripAtom, runStripSchema.parse(fixture));
  render(
    <Provider store={store}>
      <RunStrip />
    </Provider>,
  );
  expect(
    screen.getByText("doc/plans/PLAN-example.md · daemon running · 1/3 merged · 0 need attention"),
  ).toBeTruthy();
  expect(screen.getByRole("link", { name: "Open dashboard" })).toBeTruthy();
});

it("gives the compact status the full accessible name", () => {
  const store = createStore();
  store.set(runStripAtom, {
    ...runStripSchema.parse(fixture),
    daemon_running: false,
    attention: 2,
  });
  render(
    <Provider store={store}>
      <RunStrip compact />
    </Provider>,
  );
  const status = screen.getByRole("status", {
    name: "doc/plans/PLAN-example.md · daemon stopped · 1/3 merged · 2 need attention",
  });
  expect(status.textContent).toContain("2");
});

it("renders nothing without a plan", () => {
  const store = createStore();
  store.set(runStripAtom, { ...runStripSchema.parse(fixture), plan: null });
  const { container } = render(
    <Provider store={store}>
      <RunStrip />
    </Provider>,
  );
  expect(container.innerHTML).toBe("");
});
