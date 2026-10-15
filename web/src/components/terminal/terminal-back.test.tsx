import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TerminalView } from "./terminal-view";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applySnapshot } from "@/state/apply";
import {
  FakeSocket,
  fakeTimers,
  fakeFactory,
  terminalStage,
  snapshotFor,
  renderModal,
  renderView,
  waitForMount,
  well,
} from "./terminal-view-test-helpers";

afterEach(() => {
  cleanup();
  FakeSocket.instances = [];
  vi.unstubAllGlobals();
});

describe("terminal view back button", () => {
  it("shows no back button without onBack", () => {
    renderView(terminalStage(), fakeFactory().factory, fakeTimers().deps);

    expect(screen.queryByRole("button", { name: "Back to details" })).toBeNull();
  });

  it("calls onBack once from the back button", () => {
    const onBack = vi.fn();
    const store = createStore();
    applySnapshot(store, snapshotFor(terminalStage()));
    render(
      <Provider store={store}>
        <TooltipProvider>
          <TerminalView
            stage={terminalStage()}
            frame="page"
            factory={fakeFactory().factory}
            deps={fakeTimers().deps}
            onBack={onBack}
          />
        </TooltipProvider>
      </Provider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Back to details" }));

    expect(onBack).toHaveBeenCalledTimes(1);
  });
});

describe("terminal view back button in control mode", () => {
  async function inControlMode() {
    const stage = terminalStage();
    const factory = fakeFactory();
    const { router } = renderModal(
      stage,
      factory.factory,
      fakeTimers().deps,
      `/?stage=${stage.id}&view=terminal`,
    );
    await waitForMount(factory);
    const back = screen.getByRole("button", { name: "Back to details" });
    expect(back.getAttribute("title")).toBe("Back to details (Esc)");
    fireEvent.click(screen.getByRole("switch", { name: "Take control" }));
    await waitFor(() => expect(well().dataset.mode).toBe("control"));
    return { stage, router };
  }

  it("leaves Esc out of the tooltip, since Esc goes to the agent", async () => {
    await inControlMode();

    const back = screen.getByRole("button", { name: "Back to details" });
    expect(back.getAttribute("title")).toBe("Back to details");
    expect(back.getAttribute("title")).not.toContain("(Esc)");
  });

  it("returns to the stage details from the back button", async () => {
    const { stage, router } = await inControlMode();

    fireEvent.click(screen.getByRole("button", { name: "Back to details" }));

    const params = new URLSearchParams(router.state.location.search);
    expect(params.get("stage")).toBe(stage.id);
    expect(params.has("view")).toBe(false);
    expect(screen.queryByLabelText("Agent terminal")).toBeNull();
  });

  it("names both ways out of the dialog in the hint", async () => {
    await inControlMode();

    expect(screen.getByText(/leave with ← Back to details or ✕/)).toBeTruthy();
  });
});
