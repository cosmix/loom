import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeAll, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { routes } from "@/router";
import { applySnapshot } from "@/state/apply";

type RawSnapshot = typeof fixture & Record<string, unknown>;

function rawFixture(): RawSnapshot {
  return structuredClone(fixture) as RawSnapshot;
}

function renderAt(path: string, raw: unknown = fixture) {
  const store = createStore();
  applySnapshot(store, snapshotSchema.parse(raw));
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  return render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

// Each stage card also renders a `<header>`/`<footer>`; the shell's own
// chrome is whichever one is not nested inside a card.
function notCard(el: Element): boolean {
  return !el.closest(".react-flow__node-stage");
}

function shellFooter(): HTMLElement {
  const footer = screen.getAllByRole("contentinfo").find(notCard);
  if (!footer) throw new Error("the shell footer is missing");
  return footer;
}

function serverName(): string {
  const stage = fixture.status.stages.find((candidate) => candidate.id === "server");
  if (!stage) throw new Error("fixture stage server is missing");
  return stage.name;
}

beforeAll(() => {
  // jsdom has no ResizeObserver; the shadcn ScrollArea asks for one on mount.
  if (!("ResizeObserver" in window)) {
    class ResizeObserverStub {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    Object.defineProperty(window, "ResizeObserver", { value: ResizeObserverStub });
  }
});

afterEach(cleanup);

it("token flow overview shows the plan rate in the token flow panel", () => {
  renderAt("/");

  const panel = screen.getByRole("region", { name: "Token flow" });
  expect(panel.textContent).toContain("43.2k");
});

it("token flow ledger puts the tokens column between context and time", () => {
  renderAt("/ledger");

  const headers = screen.getAllByRole("columnheader").map((th) => th.textContent?.trim());
  const context = headers.indexOf("context");
  expect(context).toBeGreaterThanOrEqual(0);
  expect(headers.slice(context, context + 3)).toEqual(["context", "tokens", "time"]);
});

it("token flow stage dialog shows one lane per agent including codex jobs", () => {
  renderAt("/?stage=server");

  const dialog = screen.getByRole("dialog");
  const lanes = within(dialog).queryAllByTestId("token-lane");
  expect(lanes).toHaveLength(3);
  expect(lanes.filter((lane) => lane.textContent?.includes("codex"))).toHaveLength(1);
});

it("token flow footer shows time left for a five hour window that runs out before reset", () => {
  renderAt("/");

  const footer = shellFooter();
  expect(within(footer).getByText(/~2h07m left/)).toBeTruthy();
  expect(footer.textContent).not.toContain("lasts to reset");
});

it("token flow footer shows lasts to reset when usage is flat", () => {
  const raw = rawFixture();
  raw.status.tokens.quota_left[0] = {
    provider: "claude",
    minutes_left: null as unknown as number,
    lasts_to_reset: true,
  };
  renderAt("/", raw);

  const footer = shellFooter();
  expect(within(footer).getByText(/lasts to reset/)).toBeTruthy();
  expect(footer.textContent).not.toMatch(/~\S*m left/);
});

it("token flow ledger row and graph card show the executing stage rate", () => {
  renderAt("/ledger");
  const row = screen
    .getAllByRole("row")
    .find((candidate) =>
      candidate.querySelector('[data-label="stage"]')?.textContent?.includes("server"),
    );
  if (!row) throw new Error("the ledger row for server is missing");
  const cell = row.querySelector('[data-label="tokens"]');
  expect(cell?.textContent ?? "").toContain("43.2k/m");
  cleanup();

  renderAt("/");
  const name = serverName();
  const card = Array.from(document.body.querySelectorAll(".react-flow__node-stage")).find(
    (candidate) => candidate.textContent?.includes(name),
  );
  if (!card) throw new Error("the graph card for server is missing");
  expect(card.textContent).toContain("43.2k/m");
});

it("token flow header keeps plan total and cache hit while the sidebar owns the fresh rate", () => {
  renderAt("/");

  const header = screen.getAllByRole("banner").find(notCard);
  if (!header) throw new Error("the shell header is missing");
  const text = header.textContent ?? "";
  expect(text).not.toContain("43.2k");
  expect(text).toContain("2.23M");
  expect(text).toContain("93%");

  const panel = screen.getByRole("region", { name: "Token flow" });
  expect(panel.textContent).toContain("43.2k");
});

it("token flow snapshot without token data renders no token flow panel", () => {
  renderAt("/");
  expect(screen.getByRole("region", { name: "Token flow" })).toBeTruthy();
  cleanup();

  const raw: Record<string, unknown> & { status: Record<string, unknown> } = rawFixture();
  delete raw.status.tokens;
  renderAt("/", raw);
  expect(screen.queryByRole("region", { name: "Token flow" })).toBeNull();
  expect(screen.getByRole("region", { name: "stage graph" })).toBeTruthy();
  const cards = document.body.querySelectorAll(".react-flow__node-stage");
  expect(cards.length).toBe(fixture.status.stages.length);
});
