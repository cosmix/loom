import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { routes } from "@/router";
import { applySnapshot } from "@/state/apply";

const snapshot = snapshotSchema.parse(fixture);

function renderAt(path: string) {
  const store = createStore();
  applySnapshot(store, snapshot);
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return router;
}

afterEach(cleanup);

describe("overview route", () => {
  // Threads are not asserted here: React Flow draws edges only after it has
  // measured both cards, which jsdom's inert ResizeObserver never reports.
  // `lib/graph.test.ts` pins the edge set on the layout instead.
  it("draws a card per stage, the key, and the attention rail", () => {
    renderAt("/");

    for (const stage of snapshot.status.stages) {
      expect(screen.getAllByText(stage.name).length).toBeGreaterThan(0);
    }
    const cards = document.body.querySelectorAll(".react-flow__node-stage");
    expect(cards.length).toBe(snapshot.status.stages.length);
    const key = screen.getByRole("list", { name: "stage states in this plan" });
    expect(key.querySelectorAll("button").length).toBe(
      new Set(snapshot.status.stages.map((stage) => stage.status)).size,
    );
    expect(screen.getAllByText("ACCEPTANCE FAILED").length).toBeGreaterThan(0);
  });

  it("opens the stage dialog from the query string and moves along the thread", () => {
    const router = renderAt("/?stage=server");

    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getAllByText("Bash").length).toBeGreaterThan(0);
    // The dependency chip switches the dialog to the upstream stage.
    fireEvent.click(screen.getByRole("button", { name: /knowledge-bootstrap/ }));
    expect(router.state.location.search).toBe("?stage=knowledge-bootstrap");
    expect(screen.getAllByText("Bootstrap Knowledge").length).toBeGreaterThan(0);
  });

  it("explains an unknown stage id instead of an empty dialog", () => {
    renderAt("/?stage=nope");
    expect(screen.getByText("No such stage")).toBeTruthy();
  });

  it("shows the loom version in the footer, not the header", () => {
    renderAt("/");
    // Each stage card also renders a `<header>`/`<footer>`, so the shell's
    // own chrome is whichever one isn't nested inside a card.
    const notCard = (el: Element) => !el.closest(".react-flow__node-stage");
    const footer = screen.getAllByRole("contentinfo").find(notCard);
    const header = screen.getAllByRole("banner").find(notCard);
    expect(footer).toBeTruthy();
    expect(header).toBeTruthy();
    expect(within(footer as HTMLElement).getByText(`v${snapshot.version}`)).toBeTruthy();
    expect((header as HTMLElement).textContent).not.toContain(`v${snapshot.version}`);
  });
});
