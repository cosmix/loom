import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeAll, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageSummary } from "@/api/schema";
import { StageModal } from "@/components/stage-modal";
import type { EmulatorFactory } from "@/components/terminal/use-terminal";
import { TooltipProvider } from "@/components/ui/tooltip";
import { routes } from "@/router";
import { applySnapshot } from "@/state/apply";

function renderAt(path: string) {
  const store = createStore();
  applySnapshot(store, snapshotSchema.parse(fixture));
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  return render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

// Each stage card also renders a `<header>`; the shell's own banner is the
// one not nested inside a card.
function shellBanner(): HTMLElement {
  const banner = screen.getAllByRole("banner").find((el) => !el.closest(".react-flow__node-stage"));
  if (!banner) throw new Error("the shell header is missing");
  return banner;
}

// Matches the effort as a whole word, so "high" does not match "xhigh".
function effortRows(table: HTMLElement, effort: string): string[] {
  const word = new RegExp(`(^|[^a-z])${effort}([^a-z]|$)`);
  return within(table)
    .getAllByRole("row")
    .map((row) => row.textContent ?? "")
    .filter((text) => word.test(text));
}

function legendItem(list: HTMLElement, model: string): HTMLElement {
  const items = within(list)
    .getAllByRole("listitem")
    .filter((item) => item.textContent?.includes(model));
  expect(items).toHaveLength(1);
  return items[0];
}

function terminalStage(): StageSummary {
  const parsed = snapshotSchema.parse(fixture);
  const source = parsed.status.stages.find((stage) => stage.id === "client");
  if (!source) throw new Error("fixture stage client is missing");
  return { ...source, session_alive: true, session_backend: "tmux" };
}

function pendingFactory(): EmulatorFactory {
  return () => new Promise(() => {});
}

function renderModal(stage: StageSummary, path: string) {
  const parsed = snapshotSchema.parse(fixture);
  const store = createStore();
  applySnapshot(store, {
    ...parsed,
    terminals: true,
    status: {
      ...parsed.status,
      stages: [stage, ...parsed.status.stages.filter((candidate) => candidate.id !== stage.id)],
    },
  });
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

it("token insights panel table lists totals per model and effort", () => {
  renderAt("/");

  const panel = screen.getByRole("region", { name: "Token flow" });
  const table = within(panel).getByRole("table", { name: "token totals by model and effort" });
  expect(effortRows(table, "high").some((text) => text.includes("180k"))).toBe(true);
  expect(effortRows(table, "xhigh").some((text) => text.includes("560k"))).toBe(true);
  const medium = effortRows(table, "medium");
  expect(medium.some((text) => text.includes("1.11M") && text.includes("13.00M"))).toBe(true);
});

it("token insights panel pie has one slice per model", () => {
  renderAt("/");

  const pie = screen.getByRole("img", { name: "fresh tokens by model" });
  expect(pie.querySelectorAll("[data-model]")).toHaveLength(3);
  const legend = screen.getByRole("list", { name: "models" });
  expect(legendItem(legend, "claude-opus-5-5").textContent).toContain("33%");
  expect(legendItem(legend, "claude-sonnet-5-5").textContent).toContain("50%");
});

it("token insights stage dialog draws a trend under each chip", () => {
  renderAt("/?stage=server");

  const tokens = screen.getByRole("region", { name: "stage tokens" });
  for (const name of [
    "fresh rate, last 30 min",
    "fresh so far, last 30 min",
    "cache hit per 30 s, last 30 min",
    "agents live, last 30 min",
  ]) {
    expect(within(tokens).getAllByRole("img", { name })).toHaveLength(1);
  }
});

it("token insights header rail shows numbers without a sparkline", () => {
  renderAt("/");

  const rail = within(shellBanner()).getByRole("region", { name: "tokens" });
  const text = rail.textContent ?? "";
  expect(text).toContain("43.2k");
  expect(text).toContain("2.23M");
  expect(text).toContain("93%");
  expect(within(rail).queryAllByRole("img")).toHaveLength(0);
});

it("token insights terminal back button returns to the stage details", () => {
  const stage = terminalStage();
  const router = renderModal(stage, `/?stage=${stage.id}&view=terminal`);

  fireEvent.click(screen.getByRole("button", { name: "Back to details" }));

  const params = new URLSearchParams(router.state.location.search);
  expect(params.get("stage")).toBe(stage.id);
  expect(params.has("view")).toBe(false);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).queryByLabelText("Agent terminal")).toBeNull();
});
