import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeAll, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type ProviderQuota, type QuotaSnapshot } from "@/api/schema";
import { QuotaMeters } from "@/components/quota-meters";
import { TooltipProvider } from "@/components/ui/tooltip";
import { routes } from "@/router";
import { applySnapshot } from "@/state/apply";

const snapshot = snapshotSchema.parse(fixture);
// The fixture's generated_at, 2026-09-04T12:00:00Z: codex is 4 minutes old
// and the claude 5h window is 2h13m from its reset.
const NOW_SECS = 1788523200;

function renderMeters(quota: QuotaSnapshot, nowSecs = NOW_SECS) {
  return render(
    <TooltipProvider>
      <QuotaMeters snapshot={quota} nowSecs={nowSecs} />
    </TooltipProvider>,
  );
}

function renderAt(path: string) {
  const store = createStore();
  applySnapshot(store, snapshot);
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  return render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
}

function claudeQuota(): ProviderQuota {
  const quota = snapshot.status.quota.claude;
  if (quota === null) throw new Error("fixture has no claude quota");
  return quota;
}

function codexQuota(): ProviderQuota {
  const quota = snapshot.status.quota.codex;
  if (quota === null) throw new Error("fixture has no codex quota");
  return quota;
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

describe("QuotaMeters", () => {
  it("renders both providers, both slots each, from the fixture", () => {
    renderMeters(snapshot.status.quota);

    expect(screen.getByText("claude")).toBeTruthy();
    expect(screen.getByText("codex")).toBeTruthy();
    expect(screen.getByText("48%")).toBeTruthy();
    expect(screen.getByText("31%")).toBeTruthy();
    expect(screen.getByText("63%")).toBeTruthy();
    expect(screen.getAllByText("—")).toHaveLength(1);
    expect(screen.getAllByRole("img")).toHaveLength(4);
    expect(
      screen.getByRole("img", { name: "claude 5h window 48% used, resets in 2h13m" }),
    ).toBeTruthy();
    expect(screen.getByRole("img", { name: "codex 5h window no reading" })).toBeTruthy();
    expect(screen.queryByText(/old/)).toBeNull();
  });

  it("renders nothing when neither provider has data", () => {
    const { container } = renderMeters({ claude: null, codex: null });

    expect(container.innerHTML).toBe("");
  });

  it("marks a stale reading with its age", () => {
    renderMeters({ claude: { ...claudeQuota(), observed_at: NOW_SECS - 700 }, codex: null });

    expect(screen.getByText(/11m old/)).toBeTruthy();
    expect(
      screen.getByRole("img", {
        name: "claude 5h window 48% used, resets in 2h13m, reading 11m old",
      }),
    ).toBeTruthy();
  });

  it("keeps the last good windows next to a poll error", () => {
    renderMeters({ claude: null, codex: { ...codexQuota(), error: "rate limited" } });

    expect(screen.getByText(/rate limited/)).toBeTruthy();
    expect(screen.getByText("63%")).toBeTruthy();
  });
});

describe("footer", () => {
  it("shows the meters and the legend hint on the ledger route", () => {
    renderAt("/");

    expect(screen.getByText("48%")).toBeTruthy();
    // The footer clocks against real time, so the fixture reading is stale
    // here and the label carries an age suffix; match the prefix only.
    expect(screen.getByRole("img", { name: /^codex 5h window no reading/ })).toBeTruthy();
    expect(screen.getAllByText(/legend/).length).toBeGreaterThan(0);
  });

  it("shows the meters on a stage route", () => {
    const [first] = snapshot.status.stages;
    if (first === undefined) throw new Error("fixture has no stages");
    renderAt(`/stages/${first.id}`);

    expect(screen.getByText("48%")).toBeTruthy();
    expect(screen.getByText("63%")).toBeTruthy();
  });
});
