import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Snapshot, type StageSummary } from "@/api/schema";
import type { HistoryLoader } from "@/api/terminal-history";
import { StageModal } from "@/components/stage-modal";
import { fakeFactory } from "@/components/terminal/terminal-view-test-helpers";
import type { EmulatorFactory } from "@/components/terminal/use-terminal";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applySnapshot } from "@/state/apply";

const fixture = snapshotSchema.parse(fixtureJson);
const SESSION = "session-9c1d4e7a-1788700000";

function blockedStage(overrides: Partial<StageSummary> = {}): StageSummary {
  const source = fixture.status.stages.find((stage) => stage.id === "local-runtime");
  if (!source) throw new Error("fixture stage local-runtime is missing");
  return { ...source, ...overrides };
}

function snapshotFor(stage: StageSummary, terminals: boolean): Snapshot {
  return {
    ...structuredClone(fixture),
    terminals,
    status: {
      ...fixture.status,
      stages: [stage, ...fixture.status.stages.filter((candidate) => candidate.id !== stage.id)],
    },
  };
}

function renderModal(stage: StageSummary, path: string, terminals = true) {
  const store = createStore();
  applySnapshot(store, snapshotFor(stage, terminals));
  const history = fakeFactory();
  const asked: string[] = [];
  const load: HistoryLoader = async (session) => {
    asked.push(session);
    return new TextEncoder().encode("saved screen\n");
  };
  const live: EmulatorFactory = () => new Promise(() => {});
  const router = createMemoryRouter(
    [
      {
        path: "/",
        element: (
          <StageModal
            terminalFactory={live}
            historyFactory={history.factory}
            historyLoader={load}
          />
        ),
      },
    ],
    { initialEntries: [path] },
  );
  render(
    <Provider store={store}>
      <TooltipProvider>
        <RouterProvider router={router} />
      </TooltipProvider>
    </Provider>,
  );
  return { router, asked, doubles: history.doubles };
}

afterEach(cleanup);

describe("stage modal terminal history", () => {
  it("keeps the Terminal button enabled as Terminal history for an ended session", () => {
    renderModal(blockedStage(), "/?stage=local-runtime");

    const button = screen.getByRole("button", { name: "open terminal history" });
    expect((button as HTMLButtonElement).disabled).toBe(false);
    expect(button.textContent).toContain("Terminal history");
    expect(screen.queryByRole("button", { name: "open terminal" })).toBeNull();
  });

  it("opens the saved screen read-only, with no live connection", async () => {
    const { router, asked, doubles } = renderModal(blockedStage(), "/?stage=local-runtime");

    fireEvent.click(screen.getByRole("button", { name: "open terminal history" }));

    await waitFor(() => expect(doubles[0]?.calls.written).toHaveLength(1));
    expect(router.state.location.search).toBe("?stage=local-runtime&view=terminal");
    expect(asked).toEqual([SESSION]);
    expect(doubles[0].calls.readOnly).toEqual([true]);
    expect(screen.getByText(/read-only/)).toBeTruthy();
  });

  it("opens it with t as well", async () => {
    const { router } = renderModal(blockedStage(), "/?stage=local-runtime");

    fireEvent.keyDown(screen.getByRole("dialog"), { key: "t" });

    await waitFor(() =>
      expect(router.state.location.search).toBe("?stage=local-runtime&view=terminal"),
    );
  });

  it("prefers a live terminal when the stage has a live session", () => {
    renderModal(
      blockedStage({ session_alive: true, session_backend: "tmux" }),
      "/?stage=local-runtime",
    );

    expect(screen.getByRole("button", { name: "open terminal" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "open terminal history" })).toBeNull();
  });

  it("stays disabled without --terminals, however much history there is", () => {
    renderModal(blockedStage(), "/?stage=local-runtime", false);

    const button = screen.getByRole("button", { name: "open terminal" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });

  it("stays disabled for a stage that saved no history", () => {
    renderModal(blockedStage({ terminal_history_session: undefined }), "/?stage=local-runtime");

    const button = screen.getByRole("button", { name: "open terminal" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });

  it("lists the acceptance criteria and the whole reason in the dialog", () => {
    renderModal(blockedStage(), "/?stage=local-runtime");

    expect(screen.getByRole("region", { name: "acceptance criteria" })).toBeTruthy();
    expect(screen.getByText("what to do")).toBeTruthy();
    expect(screen.getByText(/Or allow AF_UNIX for this stage\./)).toBeTruthy();
  });
});
