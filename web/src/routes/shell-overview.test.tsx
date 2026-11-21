import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { sampleRecordJson } from "@/components/plan-overview/sample-record";
import { routes } from "@/router";
import { applySnapshot } from "@/state/apply";
import { connectionAtom } from "@/state/atoms";

const plain = snapshotSchema.parse(fixture);
const finished = snapshotSchema.parse({
  ...fixture,
  status: { ...fixture.status, plan_stats: sampleRecordJson },
});

function renderShell(path = "/ledger") {
  const store = createStore();
  applySnapshot(store, plain);
  store.set(connectionAtom, { phase: "live", since: Date.now() });
  render(
    <Provider store={store}>
      <RouterProvider router={createMemoryRouter(routes, { initialEntries: [path] })} />
    </Provider>,
  );
  return store;
}

const modals = () => document.querySelectorAll('[role="dialog"], [role="alertdialog"]');

afterEach(cleanup);

describe("shell plan overview", () => {
  it("mounts no overview while the status carries no plan_stats", () => {
    renderShell();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("mounts the overview when plan_stats arrives and drops it when the record goes", () => {
    const store = renderShell();
    act(() => applySnapshot(store, finished));
    expect(screen.getByRole("alertdialog", { name: /Sample plan/ })).toBeTruthy();
    act(() => applySnapshot(store, plain));
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("is the only modal when the stage and settings dialogs were already open", () => {
    const store = renderShell("/ledger?stage=server&settings=1");
    expect(modals().length).toBeGreaterThan(0);
    act(() => applySnapshot(store, finished));
    expect(modals()).toHaveLength(1);
    expect(screen.getByRole("alertdialog", { name: /Sample plan/ })).toBeTruthy();
  });

  it("opens no help dialog from the header or the shortcut while the overview is up", () => {
    const store = renderShell();
    act(() => applySnapshot(store, finished));
    fireEvent.click(screen.getByRole("button", { name: "open help", hidden: true }));
    fireEvent.keyDown(window, { key: "?" });
    expect(modals()).toHaveLength(1);
    expect(screen.queryByRole("dialog", { name: "Loom help" })).toBeNull();
    // Returning to a plan in progress does not bring back a help dialog opened under the lock.
    act(() => applySnapshot(store, plain));
    expect(screen.queryByRole("dialog", { name: "Loom help" })).toBeNull();
  });

  it("makes the page behind the overview inert", () => {
    const store = renderShell();
    act(() => applySnapshot(store, finished));
    const page = screen.getByRole("button", { name: "open help", hidden: true }).closest("[inert]");
    expect(page).not.toBeNull();
    act(() => applySnapshot(store, plain));
    expect(document.querySelector("[inert]")).toBeNull();
  });
});
