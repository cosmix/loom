import dagre from "@dagrejs/dagre";
import { act, cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { routes } from "@/router";
import { snapshotAtom } from "@/state/atoms";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it("reuses geometry across telemetry updates and refreshes it for topology and size", () => {
  const store = createStore();
  const snapshot = snapshotSchema.parse(fixture);
  store.set(snapshotAtom, snapshot);
  const layout = vi.spyOn(dagre, "layout");
  const router = createMemoryRouter(routes, { initialEntries: ["/"] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  const initial = layout.mock.calls.length;
  expect(initial).toBeGreaterThan(0);
  const position = document
    .querySelector('.react-flow__node-stage[data-id="knowledge-bootstrap"]')
    ?.getAttribute("style");

  const next = structuredClone(snapshot);
  next.status.stages[0].name = "Updated stage name";
  if (next.status.tokens?.stages[0]) next.status.tokens.stages[0].total.output += 1;
  act(() => store.set(snapshotAtom, next));
  expect(screen.getAllByText("Updated stage name").length).toBeGreaterThan(0);
  expect(layout).toHaveBeenCalledTimes(initial);
  expect(
    document
      .querySelector('.react-flow__node-stage[data-id="knowledge-bootstrap"]')
      ?.getAttribute("style"),
  ).toBe(position);

  const changed = structuredClone(next);
  changed.status.stages[0].dependencies = [changed.status.stages[1].id];
  act(() => store.set(snapshotAtom, changed));
  expect(layout).toHaveBeenCalledTimes(initial + 1);

  const resized = structuredClone(changed);
  Object.assign(resized.status.stages[0], {
    status: "executing",
    context_tokens: 1,
    context_ceiling_tokens: 10,
  });
  act(() => store.set(snapshotAtom, resized));
  expect(layout).toHaveBeenCalledTimes(initial + 2);

  const withoutTokens = structuredClone(resized);
  expect(withoutTokens.status.tokens?.stages.length).toBeGreaterThan(0);
  withoutTokens.status.tokens!.stages = [];
  act(() => store.set(snapshotAtom, withoutTokens));
  expect(layout).toHaveBeenCalledTimes(initial + 3);

  const removed = structuredClone(withoutTokens);
  removed.status.stages.pop();
  act(() => store.set(snapshotAtom, removed));
  expect(layout).toHaveBeenCalledTimes(initial + 4);
  act(() => store.set(snapshotAtom, withoutTokens));
  expect(layout).toHaveBeenCalledTimes(initial + 5);
});
