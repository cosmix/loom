import { cleanup, render, screen } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import chatFixture from "@/plan/api/fixtures/chat-snapshot.json";
import runFixture from "@/plan/api/fixtures/pressure-run.json";
import { chatSnapshotSchema, sessionViewSchema } from "@/plan/api/schema-chat";
import { pressureRunSchema } from "@/plan/api/schema-pressure";
import { snapshotAtom } from "./model";
import { PressureHeader } from "./pressure/pressure-header";
import { pressureRunAtom } from "./pressure/use-pressure";
import { Transcript } from "./transcript";
import { turn } from "./transcript-test-kit";

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise<Response>(() => {})),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

// The live signs of the pane: a live status dot, the block spinner, a turning arrow. The roundel is
// the header pill's alone (ui/DESIGN.md, Status), so none is drawn here. A running step turns an
// arrow; the pane has no live dot.
const busy = (root: HTMLElement) =>
  root.querySelectorAll(
    '.activity-glyph:not([data-paused="true"]) .activity-glyph__led, .plan-spinner, .plan-arrow-spinner[data-busy="true"], .aurora-roundel',
  );

function drawPane(run: unknown, state: "working" | "idle") {
  const session = sessionViewSchema.parse({
    ...chatFixture.session,
    id: "session-1",
    kind: "pressure",
    state,
  });
  const store = createStore();
  store.set(pressureRunAtom, pressureRunSchema.parse(run));
  store.set(snapshotAtom, {
    ...chatSnapshotSchema.parse(chatFixture),
    session,
    previous: null,
    queue: [],
    turns: [turn([], { open: state === "working" })],
  });
  const router = createMemoryRouter(
    [
      {
        path: "*",
        element: (
          <>
            <PressureHeader session={session} />
            <Transcript />
          </>
        ),
      },
    ],
    { initialEntries: ["/files/elsewhere.md"] },
  );
  return render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

it("a working pressure run's pane shows exactly one turning arrow: the running step's, none in the transcript", () => {
  // The fixture runs the Codex step: its card and the head rest, the row's arrow turns.
  const pane = drawPane(runFixture, "working");
  expect(busy(screen.getByRole("log"))[0] ?? null).toBeNull();
  expect(busy(pane.container)).toHaveLength(1);
});

it("steps running side by side each turn their own arrow, one per step", () => {
  const both = {
    ...runFixture,
    steps: runFixture.steps.map((step) => ({ ...step, state: "running", ended_ms: null })),
  };
  expect(busy(drawPane(both, "working").container)).toHaveLength(runFixture.steps.length);
});

it("an ended pressure run's pane shows no live sign", () => {
  const ended = {
    ...runFixture,
    steps: runFixture.steps.map((step) => ({ ...step, state: "stopped", ended_ms: 3000 })),
    ended: { kind: "stopped", by: "you", step: null, reason: null, line: null },
  };
  expect(busy(drawPane(ended, "idle").container)).toHaveLength(0);
});
