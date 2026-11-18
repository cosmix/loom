import { act, cleanup, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, beforeAll, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import type { OfferView, PlanMessage } from "@/plan/api/schema";
import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import pressureRunFixture from "@/plan/api/fixtures/pressure-run.json";
import stateFixture from "@/plan/api/fixtures/run-state.json";
import stripFixture from "@/plan/api/fixtures/run-strip.json";
import { pressureRunSchema, type PressureRunView } from "@/plan/api/schema-pressure";
import { runStripSchema } from "@/plan/api/schema-run";
import type { connectPlanSocket } from "@/plan/api/socket";
import { pressureRunAtom } from "@/plan/chat/pressure/use-pressure";
import { runOffer } from "@/plan/offers/actions";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { runDialogAtom } from "@/plan/offers/actions-run";
import { planSocketAtom } from "@/plan/state/atoms";

import { HandoffMounts } from "./handoff-mounts";
import { runStripAtom } from "./use-run";

const pressure: OfferView = {
  id: "p",
  kind: "pressure_test",
  label: "Pressure",
  target: "plan.md",
};
const run: OfferView = { id: "r", kind: "start_run", label: "Run", target: "plan.md" };

function fakeSocket() {
  const listeners = new Map<string, Set<(frame: PlanMessage) => void>>();
  const socket: ReturnType<typeof connectPlanSocket> = {
    send: () => true,
    dispose: () => {},
    subscribe: (type, handler) => {
      const set = listeners.get(type) ?? new Set();
      listeners.set(type, set);
      const listener = (frame: PlanMessage) =>
        handler(frame as Extract<PlanMessage, { type: typeof type }>);
      set.add(listener);
      return () => {
        set.delete(listener);
      };
    },
    onResync: () => () => {},
  };
  return {
    socket,
    count: (type: string) => listeners.get(type)?.size ?? 0,
    receive: (frame: PlanMessage) =>
      act(() => listeners.get(frame.type)?.forEach((listener) => listener(frame))),
  };
}

function mount() {
  const store = createStore();
  const harness = fakeSocket();
  store.set(planSocketAtom, harness.socket);
  initClient(store);
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(async (input) => {
      const url = String(input);
      if (url === "/api/pressure/options") return new Response(JSON.stringify(optionsFixture));
      if (url === "/api/run/state") return new Response(JSON.stringify(stateFixture));
      return new Response(JSON.stringify({ error: "not_found", message: url }), { status: 404 });
    }),
  );
  const view = render(
    <Provider store={store}>
      <HandoffMounts />
    </Provider>,
  );
  return { store, ...harness, ...view };
}

// The dialogs are a lazy chunk, transformed on its first import, which can outlast findBy*'s
// 1000 ms under load. Importing it here moves that cost out of the assertions' timeout.
beforeAll(() => import("@/plan/run/handoff-dialogs"));

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("wires the real subscriptions, offer actions and dialogs, and removes them on unmount", async () => {
  const { store, container, count, receive, unmount } = mount();
  expect(container.innerHTML).toBe("");
  expect(count("pressure_run")).toBe(1);
  expect(count("run_strip")).toBe(1);

  const pressureRun: PressureRunView = pressureRunSchema.parse(pressureRunFixture);
  receive({ type: "pressure_run", ...pressureRun });
  expect(store.get(pressureRunAtom)).toEqual(pressureRun);
  const strip = runStripSchema.parse(stripFixture);
  receive({ type: "run_strip", ...strip });
  expect(store.get(runStripAtom)).toEqual(strip);

  act(() => {
    expect(runOffer(pressure)).toBe(true);
  });
  expect(store.get(pressureDialogAtom)).toEqual({ open: true, plan: "plan.md" });
  expect(await screen.findByText("Pressure-test a plan")).toBeTruthy();

  act(() => {
    expect(runOffer(run)).toBe(true);
  });
  expect(store.get(runDialogAtom)).toEqual({ open: true, plan: "plan.md" });
  expect((await screen.findAllByText("Start a run")).length).toBeGreaterThan(0);

  unmount();
  expect(count("pressure_run")).toBe(0);
  expect(count("run_strip")).toBe(0);
  expect(runOffer(pressure)).toBe(false);
  expect(runOffer(run)).toBe(false);
});
