import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import helloFixture from "@/plan/api/fixtures/hello.json";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { conceptPageSchema, type EmptyState } from "@/plan/api/schema-concept";
import { summaryPanelSchema, type SummaryPanel } from "@/plan/api/schema-summaries";
import { EmptyStates } from "@/plan/concept/empty-states";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import { buildStatusOf } from "./build-status";
import { panelLoadAtom } from "./state";

const ID = "demo/product/section_04";
const ADA = { public_id: "0123456789abcdef", name: "Ada" };
const fetchMock = vi.fn<typeof fetch>();

/** The fixture's panel with nothing running, queued or failed, then `change`. */
function panel(change: Partial<SummaryPanel> = {}): SummaryPanel {
  return summaryPanelSchema.parse({
    ...panelFixture,
    running: null,
    queue: [],
    waiting: [],
    failed: [],
    question: null,
    start_error: null,
    ...change,
  });
}

/** A never-built section page for `ID`, drawn for the authoring holder with `live` as the panel. */
function showSection(
  live: SummaryPanel | null,
  kind: EmptyState["kind"] = "never_built",
  key = ID,
) {
  const store = createStore();
  initClient(store);
  store.set(youAtom, ADA);
  store.set(
    serverInfoAtom,
    serverInfoSchema.parse({ ...helloFixture.server, read_only: [], you: ADA }),
  );
  store.set(authoringAtom, {
    holder: { ...ADA, away_since: null, frees_at: null },
  });
  if (live !== null) store.set(panelLoadAtom, { status: "ready", panel: live });
  const empty: EmptyState = { kind, count: 1, offer: null, docs: [] };
  const page = conceptPageSchema.parse({
    ...overviewFixture,
    key,
    empty,
    key_chips: key === ID ? [] : overviewFixture.key_chips,
    state: null,
  });
  render(
    <Provider store={store}>
      <MemoryRouter>
        <EmptyStates page={page} empty={empty} />
      </MemoryRouter>
    </Provider>,
  );
}

function status(): string {
  return screen.getByRole("status").textContent ?? "";
}

beforeEach(() => {
  fetchMock.mockReset();
  fetchMock.mockImplementation(
    async () =>
      new Response(JSON.stringify(panel({ queue: [ID] })), {
        status: 202,
        headers: { "content-type": "application/json" },
      }),
  );
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("buildStatusOf", () => {
  it("reads running, queued, asked, waiting and failed ids, the most advanced first", () => {
    const running = panel({
      running: { ...panelFixture.running, ids: [ID], done: 0, total: 1 },
    });
    expect(buildStatusOf(running, [ID])).toEqual({
      kind: "running",
      done: 0,
      total: 1,
    });
    expect(buildStatusOf(panel({ queue: [ID] }), [ID])).toEqual({
      kind: "queued",
      behind: false,
      startError: null,
    });
    const asked = {
      id: "q1",
      count: 6,
      ids: [ID],
      estimate_tokens: 10,
      basis: "last" as const,
    };
    expect(buildStatusOf(panel({ queue: [ID], question: asked }), [ID])).toEqual({
      kind: "asking",
    });
    expect(buildStatusOf(panel({ queue: [ID], waiting: [ID] }), [ID])).toEqual({
      kind: "waiting",
    });
    const failed = panel({
      failed: [{ id: ID, error: "no seal in 10 minutes" }],
    });
    expect(buildStatusOf(failed, [ID])).toEqual({
      kind: "failed",
      error: "no seal in 10 minutes",
    });
    expect(buildStatusOf(panel(), [ID])).toEqual({ kind: "idle" });
    expect(buildStatusOf(null, [ID])).toBeNull();
  });
});

describe("a page without a summary", () => {
  // Regression: a section a draft team just wrote read "Summaries not built" while its build was
  // queued, and offered nothing when no build was coming.
  it("shows the queued build as building, not as not built", () => {
    showSection(panel({ queue: [ID] }));
    expect(status()).toContain("Building summary…");
    expect(status()).toContain("Queued to start.");
    expect(screen.queryByText(/not built/)).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("shows the running build's progress", () => {
    const running = {
      ...panelFixture.running,
      ids: [ID, "overview/demo/product"],
      done: 1,
      total: 2,
    };
    showSection(panel({ running }));
    expect(status()).toContain("Building summary…");
    expect(status()).toContain("1 of 2 built.");
  });

  it("offers Build now while the build waits for edits to stop", () => {
    showSection(panel({ queue: [ID], waiting: [ID] }));
    expect(status()).toContain("Summary queued");
    expect(screen.getByRole("button", { name: "Build now" })).toBeTruthy();
  });

  it("shows a failed build's error with Retry, which presses the summary", async () => {
    showSection(panel({ failed: [{ id: ID, error: "no seal in 10 minutes" }] }));
    expect(status()).toContain("Summary build failed");
    expect(status()).toContain("no seal in 10 minutes");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    });
    await waitFor(() => expect(fetchMock).toHaveBeenCalledOnce());
    expect(fetchMock.mock.calls[0]?.[0]).toBe("/api/summaries/rebuild");
    expect(fetchMock.mock.calls[0]?.[1]?.body).toBe(JSON.stringify({ ids: [ID] }));
  });

  it("offers one Build summary action when nothing will build it", () => {
    showSection(panel());
    expect(status()).toContain("No summary yet");
    expect(status()).toContain("1 document without a summary.");
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Build summary",
    ]);
  });

  it("reads the page's own building state before the panel loads", () => {
    showSection(null, "building");
    expect(status()).toContain("Building summary…");
  });

  it("builds a never-built overview's sections, then the overview", async () => {
    showSection(panel(), "never_built", "overview/demo/product");
    expect(status()).toContain("No summaries yet");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Build summaries" }));
    });
    await waitFor(() => expect(fetchMock).toHaveBeenCalledOnce());
    const ids = overviewFixture.key_chips.map((chip) => chip.doc.page);
    expect(fetchMock.mock.calls[0]?.[1]?.body).toBe(
      JSON.stringify({ ids: [...ids, "overview/demo/product"] }),
    );
  });
});
