import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/run-strip.json";
import { runStripSchema } from "@/plan/api/schema-run";
import { RunStrip } from "./run-strip";
import { runStripAtom } from "./use-run";

vi.mock("./dashboard-button", () => ({
  DashboardButton: ({ variant }: { variant?: string }) => (
    <a href="/dashboard" data-variant={variant}>
      Open dashboard
    </a>
  ),
}));
afterEach(cleanup);

function renderStrip(patch: object, compact = false) {
  const store = createStore();
  store.set(runStripAtom, { ...runStripSchema.parse(fixture), ...patch });
  return render(
    <Provider store={store}>
      <RunStrip compact={compact} />
    </Provider>,
  );
}

it("shows the plan's file name with the status and dashboard, and the path as its title", () => {
  renderStrip({});
  const line = screen.getByText("PLAN-example.md · daemon running · 1/3 merged · 0 need attention");
  expect(line.closest("[title]")?.getAttribute("title")).toBe(
    "doc/plans/PLAN-example.md · daemon running · 1/3 merged · 0 need attention",
  );
  expect(line.closest("[title]")?.classList.contains("truncate")).toBe(true);
  expect(line.closest("[title]")?.classList.contains("min-w-0")).toBe(true);
  expect(screen.getByRole("link", { name: "Open dashboard" }).dataset.variant).toBe("link");
});

it("shows a U+202E in the plan name as a visible marker, not a reversal", () => {
  renderStrip({ plan: "doc/plans/PLAN-‮exe.md" });
  expect(screen.getByText(/PLAN-⟨U\+202E⟩exe\.md · daemon running/)).toBeTruthy();
  expect(document.body.textContent).not.toContain("‮");
});

it("renders the compact status as a button that opens the details, beside an icon link", () => {
  renderStrip({ daemon_running: false, attention: 2 }, true);
  const trigger = screen.getByRole("button", {
    name: "Run status: daemon stopped, 2 need attention",
  });
  expect(trigger.textContent).toBe("2");
  expect(screen.getByRole("link", { name: "Open dashboard" }).dataset.variant).toBe("icon");
  expect(screen.queryByText(/daemon stopped · 1\/3 merged/)).toBeNull();

  fireEvent.click(trigger);

  const popover = screen.getByRole("dialog", { name: "Run status" });
  expect(popover.textContent).toBe(
    "doc/plans/PLAN-example.md · daemon stopped · 1/3 merged · 2 need attention",
  );
});

it("renders nothing without a plan", () => {
  const { container } = renderStrip({ plan: null });
  expect(container.innerHTML).toBe("");
});
