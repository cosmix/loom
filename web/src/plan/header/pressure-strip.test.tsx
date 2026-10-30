import { act, cleanup, render, screen } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { PressureRunView, PressureStepView } from "@/plan/api/schema-pressure";
import { pressureRunAtom } from "@/plan/chat/pressure/use-pressure";

import { PressureStrip } from "./pressure-strip";

const NOW = Date.parse("2026-10-29T10:00:00Z");

function step(kind: PressureStepView["kind"], state: PressureStepView["state"]): PressureStepView {
  return {
    round: 1,
    kind,
    session_id: null,
    model: "claude",
    effort: "high",
    state,
    started_ms: NOW - 60_000,
    ended_ms: state === "running" ? null : NOW - 30_000,
    tokens: null,
    codex: null,
    edits: [],
  };
}

function run(patch: Partial<PressureRunView> = {}): PressureRunView {
  return {
    plan: "doc/plans/PLAN-example.md",
    rounds: 2,
    round: 1,
    step: "codex",
    started_ms: NOW - 95_000,
    tokens_total: 0,
    steps: [step("claude_pressure", "done"), step("codex", "running")],
    ended: null,
    ...patch,
  };
}

function renderStrip(value: PressureRunView | null) {
  const store = createStore();
  store.set(pressureRunAtom, value);
  const view = render(
    <Provider store={store}>
      <PressureStrip />
    </Provider>,
  );
  return { store, ...view };
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
  vi.setSystemTime(NOW);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it("renders nothing without an active pressure run", () => {
  expect(renderStrip(null).container.innerHTML).toBe("");
  cleanup();
  const ended = { kind: "stopped", by: "you", step: null, reason: null, line: null } as const;
  expect(renderStrip(run({ ended })).container.innerHTML).toBe("");
});

it("names the plan, the round and step, and the finished share of steps", () => {
  const { container } = renderStrip(run());

  expect(container.textContent).toContain("Pressure-testing PLAN-example.md");
  expect(container.textContent).toContain("round 1 of 2 · Codex");
  const progress = screen.getByRole("progressbar", { name: "Pressure test progress" });
  expect(progress.getAttribute("aria-valuenow")).toBe("1");
  expect(progress.getAttribute("aria-valuemax")).toBe("6");
  expect(container.querySelector('[data-kind="pressure"]')).not.toBeNull();
});

it("counts elapsed time live and disappears when the run ends", () => {
  const { container, store } = renderStrip(run());
  expect(container.textContent).toContain("1m35s");

  act(() => vi.advanceTimersByTime(5000));
  expect(container.textContent).toContain("1m40s");

  act(() => {
    store.set(
      pressureRunAtom,
      run({ ended: { kind: "recorded", by: null, step: null, reason: null, line: null } }),
    );
  });
  expect(container.innerHTML).toBe("");
});
