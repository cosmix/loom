import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Snapshot, type StageSummary } from "@/api/schema";
import { StagePage } from "@/routes/stage";
import { TerminalPage } from "@/routes/terminal";
import { fakeFactory } from "@/components/terminal/terminal-view-test-helpers";
import type { EmulatorFactory } from "@/components/terminal/use-terminal";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applySnapshot } from "@/state/apply";

const fixture = snapshotSchema.parse(fixtureJson);

function terminalStage(): StageSummary {
  const source = fixture.status.stages.find((stage) => stage.id === "client");
  if (!source) throw new Error("fixture stage client is missing");
  return { ...source, session_alive: true };
}

function snapshotFor(stage: StageSummary): Snapshot {
  return {
    ...structuredClone(fixture),
    terminals: true,
    status: {
      ...fixture.status,
      stages: [stage, ...fixture.status.stages.filter((candidate) => candidate.id !== "client")],
    },
  };
}

function pendingFactory(): EmulatorFactory {
  return () => new Promise(() => {});
}

function renderPage(path: string, stage = terminalStage()) {
  const store = createStore();
  applySnapshot(store, snapshotFor(stage));
  const router = createMemoryRouter(
    [{ path: "/terminal/:stageId", element: <TerminalPage factory={pendingFactory()} /> }],
    { initialEntries: [path] },
  );
  render(
    <Provider store={store}>
      <TooltipProvider>
        <RouterProvider router={router} />
      </TooltipProvider>
    </Provider>,
  );
}

afterEach(cleanup);

describe("stage page terminal action", () => {
  function renderStagePage(stage: StageSummary) {
    const store = createStore();
    applySnapshot(store, snapshotFor(stage));
    const router = createMemoryRouter([{ path: "/stages/:stageId", element: <StagePage /> }], {
      initialEntries: [`/stages/${stage.id}`],
    });
    render(
      <Provider store={store}>
        <TooltipProvider>
          <RouterProvider router={router} />
        </TooltipProvider>
      </Provider>,
    );
  }
  const ended = (overrides: Partial<StageSummary> = {}): StageSummary => ({
    ...terminalStage(),
    session_alive: false,
    terminal_history_session: "session-ab12cd34-1700000000",
    ...overrides,
  });

  it("opens terminal history for an ended session", () => {
    renderStagePage(ended());

    const link = screen.getByRole("link", { name: "open terminal history" });
    expect(link.textContent).toContain("Terminal history");
  });

  it("opens the last screen of an ended fullscreen session, without the word history", () => {
    renderStagePage(ended({ terminal_history_kind: "last_screen" }));

    const link = screen.getByRole("link", { name: "open last screen" });
    expect(link.textContent).toContain("Last screen");
    expect(screen.queryByRole("link", { name: "open terminal history" })).toBeNull();
  });
});

describe("terminal route", () => {
  it("renders a known terminal in the page frame", () => {
    const stage = terminalStage();
    renderPage(`/terminal/${stage.id}`, stage);

    expect(screen.getByRole("heading", { name: stage.name })).toBeTruthy();
    expect(document.querySelector(".terminal-frame")?.getAttribute("data-frame")).toBe("page");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders No such stage for an unknown terminal id", () => {
    renderPage("/terminal/nope");

    expect(screen.getByText("No such stage")).toBeTruthy();
  });

  it("replays the saved screen of an ended session instead of attaching", async () => {
    const stage = {
      ...terminalStage(),
      session_alive: false,
      terminal_history_session: "session-ab12cd34-1700000000",
    };
    const store = createStore();
    applySnapshot(store, snapshotFor(stage));
    const history = fakeFactory();
    const router = createMemoryRouter(
      [
        {
          path: "/terminal/:stageId",
          element: (
            <TerminalPage
              factory={pendingFactory()}
              historyFactory={history.factory}
              historyLoader={async () => new TextEncoder().encode("saved\n")}
            />
          ),
        },
      ],
      { initialEntries: [`/terminal/${stage.id}`] },
    );
    render(
      <Provider store={store}>
        <TooltipProvider>
          <RouterProvider router={router} />
        </TooltipProvider>
      </Provider>,
    );

    await waitFor(() => expect(history.doubles[0]?.calls.written).toHaveLength(1));
    expect(history.doubles[0].calls.readOnly).toEqual([true]);
    expect(screen.getByText("Terminal history")).toBeTruthy();
    expect(document.querySelector(".terminal-frame")?.getAttribute("data-frame")).toBe("page");
  });
});
