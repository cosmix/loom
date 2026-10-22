import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { ActivityPanel } from "@/components/activity-panel";
import { applySnapshot } from "@/state/apply";

function renderPanel(store: ReturnType<typeof createStore>) {
  render(
    <Provider store={store}>
      <MemoryRouter>
        <ActivityPanel />
      </MemoryRouter>
    </Provider>,
  );
}

afterEach(cleanup);

describe("ActivityPanel", () => {
  it("lists server history from the first frame, with no earlier frames seen", () => {
    const store = createStore();
    applySnapshot(store, snapshotSchema.parse(fixtureJson));
    renderPanel(store);

    const items = screen.getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0].textContent).toContain("verdict applied, requeued");
    expect(items[1].textContent).toContain("entered adjudication");
  });

  it("shows the empty state when the server recorded nothing", () => {
    const store = createStore();
    const snapshot = snapshotSchema.parse(fixtureJson);
    applySnapshot(store, {
      ...snapshot,
      status: { ...snapshot.status, activity: [] },
    });
    renderPanel(store);

    expect(screen.getByText("No stage transitions yet")).toBeTruthy();
  });
});
