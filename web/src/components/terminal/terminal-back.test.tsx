import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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
  renderView,
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
