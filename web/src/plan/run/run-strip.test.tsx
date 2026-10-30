import { cleanup, render, screen } from "@testing-library/react";
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

function renderStrip(patch: object) {
  const store = createStore();
  store.set(runStripAtom, { ...runStripSchema.parse(fixture), ...patch });
  return render(
    <Provider store={store}>
      <RunStrip />
    </Provider>,
  );
}

it("shows the plan's file name with the status and dashboard, and the path as its title", () => {
  renderStrip({});
  const line = screen.getByText("PLAN-example.md · daemon running · 1/3 merged · 0 need attention");
  expect(line.closest("[title]")?.getAttribute("title")).toBe(
    "doc/plans/PLAN-example.md · daemon running · 1/3 merged · 0 need attention",
  );
  expect(screen.getByText("loom run")).toBeTruthy();
  expect(screen.getByRole("link", { name: "Open dashboard" }).dataset.variant).toBe("link");
  expect(screen.queryByRole("button")).toBeNull();
});

it("shows a U+202E in the plan name as a visible marker, not a reversal", () => {
  renderStrip({ plan: "doc/plans/PLAN-‮exe.md" });
  expect(screen.getByText(/PLAN-⟨U\+202E⟩exe\.md · daemon running/)).toBeTruthy();
  expect(document.body.textContent).not.toContain("‮");
});

it("renders nothing without a plan", () => {
  const { container } = renderStrip({ plan: null });
  expect(container.innerHTML).toBe("");
});

it.each([
  { running: true, paused: null },
  { running: false, paused: "true" },
])("renders the running glyph with daemon running=$running", ({ running, paused }) => {
  const { container } = renderStrip({ daemon_running: running });

  expect(container.textContent).toContain(running ? "daemon running" : "daemon stopped");
  const glyph = container.querySelector("[data-kind]");
  expect(glyph?.getAttribute("data-kind")).toBe("running");
  expect(glyph?.getAttribute("data-paused")).toBe(paused);
  expect(glyph?.getAttribute("aria-hidden")).toBe("true");
});

it.each([
  { merged: 0, width: "0%" },
  { merged: 1, width: "25%" },
  { merged: 4, width: "100%" },
])("shows merged progress of $merged/4", ({ merged, width }) => {
  renderStrip({ merged, total: 4 });

  const progress = screen.getByRole("progressbar", { name: "Merged stages" });
  expect(progress.getAttribute("aria-valuenow")).toBe(String(merged));
  expect(progress.getAttribute("aria-valuemin")).toBe("0");
  expect(progress.getAttribute("aria-valuemax")).toBe("4");
  expect(progress.querySelector<HTMLElement>("div")?.style.width).toBe(width);
});

it("omits progress for an empty run", () => {
  renderStrip({ merged: 0, total: 0 });

  expect(screen.queryByRole("progressbar")).toBeNull();
});

it("emphasizes attention in the warning tone", () => {
  renderStrip({ attention: 2, merged: 1, total: 4 });

  const line = screen.getByText(/PLAN-example\.md · daemon running · 1\/4 merged/);
  expect(line.closest("[title]")?.getAttribute("title")).toBe(
    "doc/plans/PLAN-example.md · daemon running · 1/4 merged · 2 need attention",
  );
  const emphasis = screen.getByText("2 need attention").closest('[class*="tone-warning"]');
  expect(emphasis?.textContent).toBe("2 need attention");
  expect(emphasis?.classList.contains("font-medium")).toBe(true);
});

it("keeps zero attention quiet", () => {
  const { container } = renderStrip({ attention: 0 });

  expect(container.querySelector('[class*="tone-warning"]')).toBeNull();
});

it("keeps an untrusted plan name escaped when attention is emphasized", () => {
  renderStrip({ plan: "doc/plans/PLAN-‮exe.md", attention: 2 });

  expect(screen.getByText(/PLAN-⟨U\+202E⟩exe\.md · daemon running/)).toBeTruthy();
  expect(screen.getByText("2 need attention")).toBeTruthy();
  expect(document.body.textContent).not.toContain("‮");
});
