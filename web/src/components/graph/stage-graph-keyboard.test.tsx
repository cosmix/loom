import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { StageGraph } from "@/components/graph/stage-graph";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applySnapshot } from "@/state/apply";

const snapshot = snapshotSchema.parse(fixture);
snapshot.terminals = true;
const stage = snapshot.status.stages.find((item) => item.id === "client")!;
stage.session_alive = true;
stage.session_backend = "tmux";

function renderGraph() {
  const store = createStore();
  applySnapshot(store, snapshot);
  const router = createMemoryRouter(
    [
      {
        path: "/",
        element: (
          <TooltipProvider>
            <StageGraph />
          </TooltipProvider>
        ),
      },
    ],
    { initialEntries: ["/"] },
  );
  const navigations: string[] = [];
  router.subscribe((state) => {
    if (state.historyAction === "PUSH") navigations.push(state.location.search);
  });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  const wrapper = document.body.querySelector<HTMLElement>(
    `.react-flow__node-stage[data-id="${stage.id}"]`,
  )!;
  return { navigations, wrapper };
}

/// jsdom has no activation behavior for keys, so model the browser's: a focused
/// button activates on Enter or Space unless its keydown was cancelled. A bare
/// keydown assertion would miss a handler that cancels the activation.
function pressKey(target: HTMLElement, key: string) {
  target.focus();
  const notCancelled = fireEvent.keyDown(target, { key });
  if (notCancelled && target.tagName === "BUTTON" && (key === "Enter" || key === " ")) {
    fireEvent.click(target);
  }
  fireEvent.keyUp(target, { key });
}

afterEach(cleanup);

describe("graph keyboard activation", () => {
  it("opens the terminal once from Enter on the terminal button", () => {
    const { navigations, wrapper } = renderGraph();

    pressKey(
      within(wrapper).getByRole("button", { name: `open terminal for ${stage.name}` }),
      "Enter",
    );

    expect(navigations).toEqual([`?stage=${stage.id}&view=terminal`]);
  });

  it("opens details once from Enter on the node wrapper", () => {
    const { navigations, wrapper } = renderGraph();

    pressKey(wrapper, "Enter");

    expect(navigations).toEqual([`?stage=${stage.id}`]);
  });

  it("opens details once from Enter on the details button", () => {
    const { navigations, wrapper } = renderGraph();

    pressKey(within(wrapper).getByRole("button", { name: `open ${stage.name}` }), "Enter");

    expect(navigations).toEqual([`?stage=${stage.id}`]);
  });

  it("toggles a state key with Space and clears it with Escape", () => {
    const { navigations } = renderGraph();
    const key = within(
      screen.getByRole("list", { name: "stage states in this plan" }),
    ).getAllByRole("button")[0]!;

    pressKey(key, " ");
    expect(key.getAttribute("aria-pressed")).toBe("true");

    pressKey(key, "Escape");
    expect(key.getAttribute("aria-pressed")).toBe("false");
    expect(navigations).toEqual([]);
  });
});
