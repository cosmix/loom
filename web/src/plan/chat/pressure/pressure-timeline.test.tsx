import { act, cleanup, render, screen, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import chatFixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import optionsFixture from "@/plan/api/fixtures/pressure-options.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import { sessionViewSchema } from "@/plan/api/schema-chat";
import {
  pressureRunSchema,
  type CodexState,
  type PressureRunView,
  type PressureStepKind,
  type PressureStepView,
  type StepState,
} from "@/plan/api/schema-pressure";
import { authoringAtom, connectionAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { CodexStatusItem } from "./codex-status";
import { PressureHeader } from "./pressure-header";
import { socketHarness } from "./pressure-test-harness";
import { installPressureSubscription, pressureRunAtom } from "./use-pressure";

vi.mock("./pressure-done", () => ({
  PressureDone: ({ run }: { run: PressureRunView }) => (
    <div data-testid="pressure-done" data-plan={run.plan} data-end={run.ended?.kind ?? "running"} />
  ),
}));

const run = pressureRunSchema.parse(runFixture);
const session = sessionViewSchema.parse({
  ...chatFixture.session,
  id: "session-1",
  kind: "pressure",
});
const disposers: (() => void)[] = [];

function step(round: number, kind: PressureStepKind, state: StepState = "done"): PressureStepView {
  return {
    ...run.steps[0]!,
    round,
    kind,
    state,
    ended_ms: state === "running" ? null : 3000,
    edits: [],
    tokens: null,
    codex: null,
  };
}

function mount(view: PressureRunView = run) {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: 4000 });
  store.set(pressureRunAtom, view);
  initClient(store);
  const socket = socketHarness(store);
  disposers.push(installPressureSubscription(store));
  const rendered = render(
    <Provider store={store}>
      <MemoryRouter>
        <PressureHeader session={session} />
      </MemoryRouter>
    </Provider>,
  );
  return { ...rendered, socket };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(4000);
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => Response.json(optionsFixture)),
  );
});

afterEach(() => {
  cleanup();
  disposers.splice(0).forEach((dispose) => dispose());
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it("groups interleaved steps into chronological rounds with reviewers before address", () => {
  mount({
    ...run,
    steps: [
      step(2, "claude_address"),
      step(1, "claude_pressure"),
      step(2, "codex"),
      step(1, "claude_address"),
      step(2, "claude_pressure"),
      step(1, "codex"),
    ],
  });

  expect(screen.getAllByRole("group").map((card) => card.getAttribute("aria-label"))).toEqual([
    "Round 1 Codex",
    "Round 1 /pressure",
    "Round 1 /address",
    "Round 2 Codex",
    "Round 2 /pressure",
    "Round 2 /address",
  ]);
  const roundOf = (name: string): HTMLElement => {
    const round = screen.getByRole("group", { name }).closest("[data-pressure-round]");
    if (!(round instanceof HTMLElement)) throw new Error(`no round element for ${name}`);
    return round;
  };
  const firstRound = roundOf("Round 1 Codex");
  const secondRound = roundOf("Round 2 Codex");
  expect(within(firstRound).getAllByRole("group")).toHaveLength(3);
  expect(within(secondRound).getAllByRole("group")).toHaveLength(3);
  expect(firstRound).not.toBe(secondRound);
});

it("lists a round's steps as one row each, in review, review, address order", () => {
  mount({
    ...run,
    steps: [step(1, "claude_pressure"), step(1, "claude_address"), step(1, "codex")],
    round: 1,
    ended: { kind: "interrupted", by: null, step: null, reason: null, line: null },
  });

  const codex = screen.getByRole("group", { name: "Round 1 Codex" });
  const claude = screen.getByRole("group", { name: "Round 1 /pressure" });
  const address = screen.getByRole("group", { name: "Round 1 /address" });
  expect(codex.getAttribute("class")).toContain("grid-cols-[20px_minmax(0,1fr)_auto]");
  expect(codex.parentElement).toBe(claude.parentElement);
  expect(codex.nextElementSibling).toBe(claude);
  expect(claude.nextElementSibling).toBe(address);
  expect(screen.getByText("Round 1").className).toContain("uppercase");
});

it("derives queued rows for the missing steps, later rounds and the record line", () => {
  mount({ ...run, round: 1, rounds: 2, steps: [step(1, "claude_pressure", "running")] });

  const first = document.querySelector("[data-pressure-round='1']") as HTMLElement;
  const second = document.querySelector("[data-pressure-round='2']") as HTMLElement;
  expect(
    within(first)
      .getAllByText("queued")
      .map((node) => node.parentElement?.textContent),
  ).toEqual(["Codex reviews the planqueued", "Claude addresses both reviewsqueued"]);
  expect(within(second).getAllByText("queued")).toHaveLength(3);
  expect(screen.getByText("Write the record line").textContent).toBe("Write the record linequeued");
  expect(screen.getAllByRole("group")).toHaveLength(1);
});

it("shows the record line as done once recorded and drops queued rows when a run stops", () => {
  const { container } = mount({
    ...run,
    round: 1,
    rounds: 1,
    steps: [step(1, "codex"), step(1, "claude_pressure"), step(1, "claude_address")],
    ended: { kind: "recorded", by: null, step: null, reason: null, line: "ok" },
  });
  const record = screen.getByText("Write the record line").parentElement as HTMLElement;
  expect(within(record).getByText("done")).toBeTruthy();
  expect(container.querySelector(".lucide-circle-dashed")).toBeNull();
  cleanup();

  mount({
    ...run,
    round: 1,
    rounds: 2,
    steps: [step(1, "codex", "stopped")],
    ended: { kind: "stopped", by: "Ada", step: null, reason: null, line: null },
  });
  expect(screen.queryByText("queued")).toBeNull();
  expect(screen.queryByText("Write the record line")).toBeNull();
});

it("renders only the steps received for a partially completed round", () => {
  mount({ ...run, steps: [step(2, "claude_pressure", "running")] });

  expect(screen.getAllByRole("group").map((card) => card.getAttribute("aria-label"))).toEqual([
    "Round 2 /pressure",
  ]);
});

it.each<[PressureStepKind, string]>([
  ["codex", "Codex"],
  ["claude_pressure", "/pressure"],
  ["claude_address", "/address"],
])("shows the turning arrow for a running %s step", (kind, label) => {
  mount({ ...run, round: 1, step: kind, steps: [step(1, kind, "running")] });

  const card = screen.getByRole("group", { name: `Round 1 ${label}` });
  const arrow = card.querySelector(".plan-arrow-spinner");
  expect(arrow?.getAttribute("data-busy")).toBe("true");
  expect(arrow?.getAttribute("aria-hidden")).toBe("true");
  expect(card.querySelector(".activity-glyph")).toBeNull();
});

it("turns the arrow of a Codex step running beside the /pressure step the run is on", () => {
  mount({
    ...run,
    step: "claude_pressure",
    steps: [step(1, "claude_pressure", "running"), step(1, "codex", "running")],
  });

  for (const label of ["Codex", "/pressure"]) {
    const card = screen.getByRole("group", { name: `Round 1 ${label}` });
    expect(card.querySelector('.plan-arrow-spinner[data-busy="true"]')).not.toBeNull();
  }
});

it.each<[StepState, string]>([
  ["done", ".lucide-circle-check"],
  ["failed", ".lucide-x"],
  ["stopped", ".lucide-x"],
])("shows a static state glyph for a %s step", (state, selector) => {
  mount({ ...run, steps: [step(1, "claude_pressure", state)] });

  const card = screen.getByRole("group", { name: "Round 1 /pressure" });
  expect(card.querySelector("[data-kind]")).toBeNull();
  expect(card.querySelector(selector)?.getAttribute("aria-hidden")).toBe("true");
});

it.each<StepState>(["running", "done", "failed", "stopped"])(
  "tells assistive technology that a step is %s",
  (state) => {
    mount({ ...run, steps: [step(1, "claude_pressure", state)] });

    const card = screen.getByRole("group", { name: "Round 1 /pressure" });
    const hidden = within(card).getByText(state);
    expect(hidden.className).toContain("sr-only");
  },
);

it("draws the header flask still while the run works and after a socket update ends it", () => {
  const { container, socket } = mount();
  const head = container.querySelector(".chat-run-header");
  expect(head?.querySelector(".lucide-flask-conical")).not.toBeNull();
  expect(head?.querySelector(".activity-glyph, .plan-arrow-spinner")).toBeNull();

  act(() =>
    socket.receive({
      ...run,
      ended: { kind: "interrupted", by: null, step: null, reason: null, line: null },
    }),
  );

  expect(head?.querySelector(".activity-glyph, .plan-arrow-spinner")).toBeNull();
  expect(screen.getByText("Interrupted")).toBeTruthy();
});

it("mounts PressureDone before the timeline and the end line with the latest run", () => {
  const { socket } = mount({ ...run, steps: [step(2, "claude_address", "running")] });
  const done = screen.getByTestId("pressure-done");
  expect(done.getAttribute("data-plan")).toBe(run.plan);
  expect(done.getAttribute("data-end")).toBe("running");
  const timeline = done.nextElementSibling;
  expect(timeline?.hasAttribute("data-pressure-timeline")).toBe(true);
  expect(timeline?.nextElementSibling).toBeNull();
  expect(screen.getByRole("button", { name: "Stop" })).toBeTruthy();

  act(() =>
    socket.receive({
      ...run,
      steps: [step(2, "claude_address")],
      ended: { kind: "recorded", by: null, step: null, reason: null, line: "rounds complete" },
    }),
  );

  const address = screen.getByRole("group", { name: "Round 2 /address" });
  const recorded = address.closest("[data-pressure-timeline]");
  expect(recorded?.previousElementSibling).toBe(done);
  expect(done.getAttribute("data-end")).toBe("recorded");
  expect(recorded?.nextElementSibling).toBe(
    screen.getByText("Pressure test recorded · rounds complete").closest("p"),
  );
});

it.each<CodexState>(["running", "review_written", "no_review", "failed"])(
  "draws no dot or arrow in the Codex card, whatever its state (%s)",
  (state) => {
    const { container } = render(
      <CodexStatusItem
        status={{ model: "gpt-6.1-terra", state, report_path: null, log_tail: null }}
        startedMs={2000}
      />,
    );

    expect(container.querySelector(".activity-glyph, .plan-arrow-spinner")).toBeNull();
    if (state === "running") {
      expect(screen.getByRole("status").textContent).toBe("Codex is reviewing");
    }
  },
);
