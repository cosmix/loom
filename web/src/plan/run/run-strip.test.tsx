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

function renderStrip(patch: object, compact = false, variant?: "bar" | "row") {
  const store = createStore();
  store.set(runStripAtom, { ...runStripSchema.parse(fixture), ...patch });
  return render(
    <Provider store={store}>
      <RunStrip compact={compact} variant={variant} />
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

it.each([
  { compact: false, running: true, paused: null, dimmed: false },
  { compact: false, running: false, paused: "true", dimmed: false },
  { compact: true, running: true, paused: null, dimmed: false },
  { compact: true, running: false, paused: "true", dimmed: true },
])(
  "renders the running glyph with daemon running=$running and compact=$compact",
  ({ compact, running, paused, dimmed }) => {
    const { container } = renderStrip({ daemon_running: running }, compact);

    const glyph = container.querySelector("[data-kind]");
    expect(glyph?.getAttribute("data-kind")).toBe("running");
    expect(glyph?.getAttribute("data-paused")).toBe(paused);
    expect(glyph?.getAttribute("aria-hidden")).toBe("true");
    expect(glyph?.classList.contains("opacity-50")).toBe(dimmed);
    if (compact) {
      expect(screen.getByRole("button").contains(glyph)).toBe(true);
    }
  },
);

it.each([
  { compact: false, merged: 0, width: "0%" },
  { compact: false, merged: 1, width: "25%" },
  { compact: false, merged: 4, width: "100%" },
  { compact: true, merged: 0, width: "0%" },
  { compact: true, merged: 1, width: "25%" },
  { compact: true, merged: 4, width: "100%" },
])("shows merged progress of $merged/4 with compact=$compact", ({ compact, merged, width }) => {
  renderStrip({ merged, total: 4 }, compact);

  const progress = screen.getByRole("progressbar", { name: "Merged stages" });
  expect(progress.getAttribute("aria-valuenow")).toBe(String(merged));
  expect(progress.getAttribute("aria-valuemin")).toBe("0");
  expect(progress.getAttribute("aria-valuemax")).toBe("4");
  expect(progress.querySelector<HTMLElement>("div")?.style.width).toBe(width);
});

it.each([false, true])("omits progress for an empty run with compact=%s", (compact) => {
  renderStrip({ merged: 0, total: 0 }, compact);

  expect(screen.queryByRole("progressbar")).toBeNull();
});

it.each([false, true])("emphasizes attention in the permission tone with compact=%s", (compact) => {
  renderStrip({ attention: 2 }, compact);
  if (compact) {
    const trigger = screen.getByRole("button", {
      name: "Run status: daemon running, 2 need attention",
    });
    expect(screen.getByText("2").classList.contains("text-(--tone-warning)")).toBe(true);
    fireEvent.click(trigger);
  }

  const attention = screen.getByText("2 need attention");
  const emphasis = attention.closest('[class*="tone-warning"]');
  expect(emphasis?.textContent).toBe("2 need attention");
  expect(emphasis?.classList.contains("font-medium")).toBe(true);
});

it.each([false, true])("keeps zero attention quiet with compact=%s", (compact) => {
  const { container } = renderStrip({ attention: 0 }, compact);

  expect(container.querySelector('[class*="tone-warning"]')).toBeNull();
});

it.each([false, true])("preserves the compact button name with daemon running=%s", (running) => {
  renderStrip({ daemon_running: running, attention: 2 }, true);

  const name = running
    ? "Run status: daemon running, 2 need attention"
    : "Run status: daemon stopped, 2 need attention";
  expect(screen.getByRole("button", { name }).textContent).toBe("2");
});

it("keeps an untrusted plan name escaped when attention is emphasized", () => {
  renderStrip({ plan: "doc/plans/PLAN-‮exe.md", attention: 2 });

  expect(screen.getByText(/PLAN-⟨U\+202E⟩exe\.md · daemon running/)).toBeTruthy();
  expect(screen.getByText("2 need attention")).toBeTruthy();
  expect(document.body.textContent).not.toContain("‮");
});

it("renders the row variant with the status line, a dashboard link and merged progress", () => {
  const { container } = renderStrip({ attention: 2, merged: 1, total: 4 }, true, "row");

  expect(screen.getByText("loom run")).toBeTruthy();
  const line = screen.getByText(/PLAN-example\.md · daemon running · 1\/4 merged/);
  expect(line.closest("[title]")?.getAttribute("title")).toBe(
    "doc/plans/PLAN-example.md · daemon running · 1/4 merged · 2 need attention",
  );
  expect(screen.getByText("2 need attention").closest('[class*="tone-warning"]')).not.toBeNull();
  expect(screen.getByRole("link", { name: "Open dashboard" }).dataset.variant).toBe("link");
  expect(screen.queryByRole("button")).toBeNull();
  expect(
    screen.getByRole("progressbar", { name: "Merged stages" }).getAttribute("aria-valuenow"),
  ).toBe("1");
  expect(container.querySelector('[data-kind="running"]')?.hasAttribute("data-paused")).toBe(false);
});

it("pauses the row's running glyph while the daemon is stopped", () => {
  const { container } = renderStrip({ daemon_running: false }, false, "row");

  expect(container.querySelector('[data-kind="running"]')?.getAttribute("data-paused")).toBe(
    "true",
  );
  expect(container.textContent).toContain("daemon stopped");
});

it("renders nothing for the row variant without a plan", () => {
  expect(renderStrip({ plan: null }, false, "row").container.innerHTML).toBe("");
});
