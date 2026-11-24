import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Snapshot, type StageSummary } from "@/api/schema";
import type { HistoryLoader } from "@/api/terminal-history";
import { fakeFactory } from "@/components/terminal/terminal-view-test-helpers";
import type { EmulatorFactory } from "@/components/terminal/use-terminal";
import { TooltipProvider } from "@/components/ui/tooltip";
import { TerminalPage } from "@/routes/terminal";
import { applySnapshot } from "@/state/apply";

const fixture = snapshotSchema.parse(fixtureJson);
const SESSION = "session-ab12cd34-1700000000";
const SAVED = "\x1b[32mLAST-ALT-FRAME\x1b[0m\n";

/// The fixture's client stage after its fullscreen tmux session ended and left
/// its last screen.
function endedFullscreenStage(): StageSummary {
  const source = fixture.status.stages.find((stage) => stage.id === "client");
  if (!source) throw new Error("fixture stage client is missing");
  return {
    ...source,
    session_alive: false,
    terminal_history_session: SESSION,
    terminal_history_kind: "last_screen",
  } as StageSummary;
}

function snapshotFor(stage: StageSummary): Snapshot {
  return {
    ...structuredClone(fixture),
    terminals: true,
    status: {
      ...fixture.status,
      stages: [stage, ...fixture.status.stages.filter((candidate) => candidate.id !== stage.id)],
    },
  };
}

afterEach(cleanup);

describe("terminal route last screen", () => {
  it("replays the saved capture read only under last screen labels", async () => {
    const stage = endedFullscreenStage();
    const store = createStore();
    applySnapshot(store, snapshotFor(stage));
    const attach = vi.fn<EmulatorFactory>(() => new Promise(() => {}));
    const history = fakeFactory();
    const loader = vi.fn<HistoryLoader>(async () => new TextEncoder().encode(SAVED));
    const router = createMemoryRouter(
      [
        {
          path: "/terminal/:stageId",
          element: (
            <TerminalPage
              factory={attach}
              historyFactory={history.factory}
              historyLoader={loader}
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
    expect(loader).toHaveBeenCalledWith(SESSION);
    expect(new TextDecoder().decode(history.doubles[0].calls.written[0])).toBe(SAVED);
    expect(history.doubles[0].calls.readOnly).toEqual([true]);
    expect(attach).not.toHaveBeenCalled();
    expect(document.querySelector(".eyebrow")?.textContent).toBe("Last screen");
    expect(screen.getByLabelText("Saved last screen")).toBeTruthy();
    expect(screen.queryByText("Terminal history")).toBeNull();
    expect(screen.queryByLabelText("Saved terminal history")).toBeNull();
  });
});
