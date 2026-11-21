// Frozen contract for the locked plan overview.
//
// The record mirrors the Rust `PlanStatsRecord` as serde writes it: every
// nullable field present as null, and `status` and `stage_type` in the
// stages' serde names. It reaches the shell as a full `/ws` frame after the
// help dialog has already opened.
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { parseFrame } from "@/api/frames";
import { routes } from "@/router";
import { applyFrame } from "@/state/apply";
import { connectionAtom } from "@/state/atoms";
import type { Store } from "@/state/store";

const usage = (input: number, cacheWrite: number, cacheRead: number, output: number) => ({
  input,
  cache_write: cacheWrite,
  cache_read: cacheRead,
  output,
});

const record = {
  schema_version: 1,
  plan_id: "20261121T090000Z-plan-overview-contract-0b8f5d2e-4c1a-4a57-9f6e-2d7c1b3a9e10",
  plan_name: "Overview contract plan",
  started_at: "2026-11-21T09:00:00Z",
  ended_at: "2026-11-21T10:00:00Z",
  wall_secs: 3600,
  execution_secs: 2400,
  totals: usage(1200, 3400, 56000, 780),
  stages: [
    {
      id: "server",
      name: "Rust server",
      stage_type: "standard",
      status: "completed",
      started_at: "2026-11-21T09:00:00Z",
      completed_at: "2026-11-21T09:40:00Z",
      execution_secs: 2400,
      model: "opus",
      effort: "high",
      phases: [
        { phase: "contract", started_at: "2026-11-21T09:00:00Z", ended_at: "2026-11-21T09:10:00Z" },
        {
          phase: "executing",
          started_at: "2026-11-21T09:10:00Z",
          ended_at: "2026-11-21T09:40:00Z",
        },
      ],
      tokens: usage(1200, 3400, 56000, 780),
    },
    {
      id: "docs",
      name: "Documentation",
      stage_type: "standard",
      status: "skipped",
      started_at: null,
      completed_at: null,
      execution_secs: 0,
      model: null,
      effort: null,
      phases: [],
      tokens: usage(0, 0, 0, 0),
    },
  ],
  models: [
    {
      model: "opus",
      effort: "high",
      lane: "claude",
      agents: 2,
      tokens: usage(1000, 3000, 50000, 700),
    },
    {
      model: "gpt-5.6-terra",
      effort: null,
      lane: "codex",
      agents: 1,
      tokens: usage(200, 400, 6000, 80),
    },
  ],
};

function applyFull(store: Store, snapshot: unknown) {
  const parsed = parseFrame({ t: "full", snapshot });
  if (!parsed.success) throw new Error(JSON.stringify(parsed.issues));
  act(() => applyFrame(store, parsed.data));
}

function modals() {
  return document.querySelectorAll('[role="dialog"], [role="alertdialog"]');
}

function expectSoleFocusedOverview(overview: HTMLElement) {
  expect(screen.getByRole("alertdialog", { name: /Overview contract plan/ })).toBe(overview);
  expect(modals()).toHaveLength(1);
  expect(overview.contains(document.activeElement)).toBe(true);
}

afterEach(cleanup);

it("plan overview dialog stays open on escape and outside clicks and offers no close control", async () => {
  const store = createStore();
  applyFull(store, fixture);
  store.set(connectionAtom, { phase: "live", since: Date.now() });
  const router = createMemoryRouter(routes, { initialEntries: ["/ledger"] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "open help" }));
  expect(screen.getByRole("dialog", { name: "Loom help" })).toBeTruthy();

  applyFull(store, { ...fixture, status: { ...fixture.status, plan_stats: record } });
  const overview = screen.getByRole("alertdialog", { name: /Overview contract plan/ });
  // Outside-pointer listeners attach on the next task after a layer mounts.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  expectSoleFocusedOverview(overview);
  expect(within(overview).queryByRole("button", { name: /close/i })).toBeNull();

  fireEvent.keyDown(document.activeElement ?? overview, { key: "Escape" });
  expectSoleFocusedOverview(overview);

  const outside = screen.getByRole("button", { name: "open help", hidden: true });
  fireEvent.pointerDown(outside);
  fireEvent.mouseDown(outside);
  fireEvent.focusIn(outside);
  fireEvent.pointerUp(outside);
  fireEvent.mouseUp(outside);
  fireEvent.click(outside);
  expectSoleFocusedOverview(overview);

  fireEvent.keyDown(document.activeElement ?? overview, { key: "?" });
  expectSoleFocusedOverview(overview);
});
