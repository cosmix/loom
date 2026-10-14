import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Snapshot, type StageSummary } from "@/api/schema";
import { StageModal } from "@/components/stage-modal";
import type { EmulatorFactory } from "@/components/terminal/use-terminal";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applySnapshot } from "@/state/apply";

const fixture = snapshotSchema.parse(fixtureJson);

function terminalStage(overrides: Partial<StageSummary> = {}): StageSummary {
  const source = fixture.status.stages.find((stage) => stage.id === "client");
  if (!source) throw new Error("fixture stage client is missing");
  return { ...source, session_alive: true, session_backend: "tmux", ...overrides };
}

function snapshotFor(
  stage: StageSummary,
  terminals = true,
  attention?: Snapshot["attention"],
): Snapshot {
  return {
    ...structuredClone(fixture),
    terminals,
    ...(attention ? { attention } : {}),
    status: {
      ...fixture.status,
      stages: [stage, ...fixture.status.stages.filter((candidate) => candidate.id !== "client")],
    },
  };
}

function pendingFactory(): EmulatorFactory {
  return () => new Promise(() => {});
}

function renderModal(
  stage: StageSummary,
  path: string,
  terminals = true,
  attention?: Snapshot["attention"],
) {
  const store = createStore();
  applySnapshot(store, snapshotFor(stage, terminals, attention));
  const router = createMemoryRouter(
    [{ path: "/", element: <StageModal terminalFactory={pendingFactory()} /> }],
    { initialEntries: [path] },
  );
  render(
    <Provider store={store}>
      <TooltipProvider>
        <RouterProvider router={router} />
      </TooltipProvider>
    </Provider>,
  );
  return router;
}

function terminalButton(): HTMLButtonElement {
  const button = screen.getByRole("button", { name: "open terminal" });
  if (!(button instanceof HTMLButtonElement)) throw new Error("terminal button is missing");
  return button;
}

afterEach(cleanup);

describe("stage modal", () => {
  it("renders the terminal face wide for a terminal query and details without one", () => {
    const stage = terminalStage();
    renderModal(stage, `/?stage=${stage.id}&view=terminal`);
    const dialog = screen.getByRole("dialog");

    expect(screen.getByText("Terminal")).toBeTruthy();
    expect(dialog.className).toContain("terminal-dialog");
    expect(dialog.className).toContain("sm:max-w-[min(96vw,1280px)]");
    cleanup();

    renderModal(stage, `/?stage=${stage.id}`);
    expect(screen.getByText(stage.name)).toBeTruthy();
    expect(screen.queryByText("every key reaches the agent")).toBeNull();
  });

  it("opens the terminal with t only when the stage passes the terminal gate", async () => {
    const stage = terminalStage();
    const router = renderModal(stage, `/?stage=${stage.id}`);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "t" });
    await waitFor(() =>
      expect(router.state.location.search).toBe(`?stage=${stage.id}&view=terminal`),
    );
    cleanup();

    const native = terminalStage({ session_backend: "native" });
    const blocked = renderModal(native, `/?stage=${native.id}`);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "t" });

    expect(blocked.state.location.search).toBe(`?stage=${native.id}`);
  });

  it.each([
    [terminalStage({ session_backend: "native" }), true],
    [terminalStage(), false],
  ])("disables unavailable terminals without a tooltip", (stage, terminals) => {
    renderModal(stage, `/?stage=${stage.id}`, terminals);
    const button = terminalButton();

    expect(button.disabled).toBe(true);
    fireEvent.pointerMove(button.parentElement!);
    expect(document.querySelector('[data-slot="tooltip-content"]')).toBeNull();
  });

  it("renders a long completion summary and its next action", () => {
    const summary =
      "Completion verification could not prove that the outgoing writer owns the expected commit after repeated boundary checks across the current session state.";
    const nextAction = "Confirm the commit owner, then retry completion verification.";
    const stage = terminalStage({
      status: "needs-human-review",
      completion_blocker: {
        state: "ownership_unknown",
        fingerprint: "fp-owner",
        failure_code: "writer-unconfirmed",
        summary,
        commit: "abc123",
        repeat_count: 2,
        first_observed_at: "2026-09-14T08:00:00Z",
        last_observed_at: "2026-09-14T08:05:00Z",
        next_action: nextAction,
      },
    });

    renderModal(stage, `/?stage=${stage.id}`);

    expect(screen.getAllByText(`completion blocked, writer unconfirmed: ${summary}`).length).toBe(
      1,
    );
    expect(screen.getByTitle(nextAction)).toBeTruthy();
    expect(screen.getByText(nextAction)).toBeTruthy();
  });

  it("shows the summary and never the agent-facing description", () => {
    const description = "Agent-facing brief. ".repeat(20).trim();
    const stage = terminalStage({ summary: "Adds the X.", description });
    renderModal(stage, `/?stage=${stage.id}`);

    expect(screen.getByText("Adds the X.")).toBeTruthy();
    expect(screen.queryByText(description)).toBeNull();
    cleanup();

    renderModal({ ...stage, summary: null }, `/?stage=${stage.id}`);
    expect(screen.queryByText("Adds the X.")).toBeNull();
    expect(screen.queryByText(description)).toBeNull();
  });

  it("keeps only the counts in the hazard header and the reason in the review notes", () => {
    const reason = "acceptance criterion 3 disputed twice; ".repeat(12).trim();
    const stage = terminalStage({
      status: "needs-adjudication",
      review_reason: reason.slice(0, 200),
      review_notes: reason,
    });
    const entry: Snapshot["attention"][number] = {
      id: stage.id,
      name: stage.name,
      label: "NEEDS ADJUDICATION",
      command: null,
      command_when: null,
      alternatives: [],
      note: "a judge session is ruling on the open disputes",
      automatic: true,
      failure_type: null,
      failure_label: null,
      evidence: [],
      review_reason: reason.slice(0, 200),
      cleanup_warning: null,
      has_human_review_choices: false,
      dispute_count: 2,
      judge_heartbeat_secs: null,
    };
    renderModal(stage, `/?stage=${stage.id}`, true, [entry]);

    const counts = screen.getByText("2 disputed");
    expect(counts.className).not.toContain("truncate");
    expect(counts.textContent).not.toContain("acceptance criterion");
    expect(screen.getByText(reason)).toBeTruthy();
    expect(screen.getAllByText(reason)).toHaveLength(1);
  });

  it("heads an automatic entry with what loom is doing and an operator entry with what to do", () => {
    const stage = terminalStage();
    const operator = fixture.attention.find((entry) => entry.id === "client");
    if (!operator) throw new Error("fixture attention entry client is missing");
    renderModal(stage, `/?stage=${stage.id}`, true, [operator]);

    expect(screen.getByText("what to do")).toBeTruthy();
    expect(screen.queryByText("what loom is doing")).toBeNull();
    cleanup();

    const note = "auto-retry 2 of 3 pending after a crash";
    renderModal(stage, `/?stage=${stage.id}`, true, [
      { ...operator, label: "BLOCKED", command: null, note, automatic: true },
    ]);

    expect(screen.getByText("what loom is doing")).toBeTruthy();
    expect(screen.queryByText("what to do")).toBeNull();
    expect(screen.getByText(note)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^copy / })).toBeNull();
  });
});
