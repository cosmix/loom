import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import helloFixture from "@/plan/api/fixtures/hello.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import questionFixture from "@/plan/api/fixtures/summary-question.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import { summaryPanelSchema } from "@/plan/api/schema-summaries";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { panelLoadAtom } from "@/plan/summaries/state";

import { LedgerRows } from "./header-ledger";
import { OverviewSummariesRow } from "./overview-summaries";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const idlePanel = {
  ...panelFixture,
  running: null,
  queue: [],
  waiting: [],
  incomplete: [],
  failed: [],
  question: null,
};

const staleChip = overviewFixture.key_chips[0]!;
const currentChips = overviewFixture.key_chips.map((chip) => ({ ...chip, out_of_date: null }));

/** The overview page, its sources out of date or not, in a store holding authoring. */
function show(panel: Record<string, unknown>, opts: { stale?: boolean; author?: boolean } = {}) {
  const { stale = true, author = true } = opts;
  const store = createStore();
  const you = helloFixture.you;
  store.set(youAtom, you);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...helloFixture.server, read_only: [] }));
  store.set(authoringAtom, {
    holder: author
      ? { ...you, away_since: null, frees_at: null }
      : { public_id: "other-browser", name: "Dana", away_since: null, frees_at: null },
  });
  store.set(panelLoadAtom, { status: "ready", panel: summaryPanelSchema.parse(panel) });
  initClient(store);
  const page = conceptPageSchema.parse({
    ...overviewFixture,
    key_chips: stale ? [staleChip, ...currentChips.slice(1)] : currentChips,
  });
  render(
    <Provider store={store}>
      <LedgerRows>
        <OverviewSummariesRow page={page} />
      </LedgerRows>
    </Provider>,
  );
  return { store, row: screen.getByRole("region", { name: "Summaries" }) };
}

describe("the Summaries row of an overview", () => {
  it("says the summaries are up to date when no source is out of date", () => {
    const { row } = show(idlePanel, { stale: false });
    expect(within(row).getByRole("status").textContent).toBe("Summaries up to date");
    expect(within(row).queryByRole("button")).toBeNull();
  });

  it("counts the out-of-date sources and offers Rebuild", () => {
    const { row } = show(idlePanel);
    expect(within(row).getByText("1 summary out of date")).toBeTruthy();
    expect(within(row).getByRole("button", { name: "Rebuild" })).toBeTruthy();
  });

  it("shows a running rebuild as its count with a progress line", () => {
    const { row } = show({ ...idlePanel, running: panelFixture.running });
    expect(within(row).getByText("Rebuilding 1 of 3")).toBeTruthy();
    const bar = within(row).getByRole("progressbar", { name: "Summaries rebuilt" });
    expect(bar.getAttribute("aria-valuenow")).toBe("1");
    expect(bar.getAttribute("aria-valuemax")).toBe("3");
  });

  it("shows a failed build with its error and Retry", () => {
    const { row } = show({
      ...idlePanel,
      failed: [{ id: "demo/product/section_01", error: "no seal in 10 minutes" }],
    });
    expect(within(row).getByText("Summaries build failed")).toBeTruthy();
    expect(within(row).getByText("no seal in 10 minutes")).toBeTruthy();
    expect(within(row).getByRole("button", { name: "Retry" })).toBeTruthy();
  });

  it("draws a rebuild waiting for approval as the count, the reason and two answers", async () => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(Response.json(idlePanel));
    vi.stubGlobal("fetch", fetchMock);
    const { row } = show({ ...idlePanel, question: questionFixture });
    expect(within(row).getByText("6 summaries out of date")).toBeTruthy();
    expect(within(row).getByText(/above the budget and waiting for approval/)).toBeTruthy();
    expect(within(row).getByRole("button", { name: "Rebuild all" })).toBeTruthy();
    fireEvent.click(within(row).getByRole("button", { name: "Not now" }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toEqual({
      question: questionFixture.id,
      rebuild: false,
    });
  });

  it("shows a browser that cannot answer the words alone", () => {
    const { row } = show({ ...idlePanel, question: questionFixture }, { author: false });
    expect(within(row).getByText(/waiting for approval/)).toBeTruthy();
    expect(within(row).queryByRole("button")).toBeNull();
  });

  it("draws no row for an overview without a summary: its body owns the build", () => {
    const page = conceptPageSchema.parse({
      ...overviewFixture,
      empty: { kind: "never_built", count: 3, offer: null, docs: [] },
    });
    render(
      <Provider store={createStore()}>
        <OverviewSummariesRow page={page} />
      </Provider>,
    );
    expect(screen.queryByRole("region", { name: "Summaries" })).toBeNull();
  });
});
