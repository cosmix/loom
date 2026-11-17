import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import helloFixture from "@/plan/api/fixtures/hello.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import questionFixture from "@/plan/api/fixtures/summary-question.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { conceptPageSchema, type ConceptPage } from "@/plan/api/schema-concept";
import { summaryPanelSchema } from "@/plan/api/schema-summaries";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { panelLoadAtom } from "@/plan/summaries/state";

import { PageHeading } from "./page-heading";

beforeEach(() => {
  vi.stubGlobal("fetch", () =>
    Promise.resolve(Response.json({ error: "not_found", message: "x" }, { status: 404 })),
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const idle = { ...panelFixture, running: null, queue: [], waiting: [], failed: [], question: null };

function show(
  overrides: Partial<ConceptPage>,
  opts: { question?: boolean; author?: boolean } = {},
) {
  const { question = false, author = true } = opts;
  const store = createStore();
  const you = helloFixture.you;
  store.set(youAtom, you);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...helloFixture.server, read_only: [] }));
  store.set(authoringAtom, {
    holder: author
      ? { ...you, away_since: null, frees_at: null }
      : { public_id: "other-browser", name: "Dana", away_since: null, frees_at: null },
  });
  store.set(panelLoadAtom, {
    status: "ready",
    panel: summaryPanelSchema.parse({ ...idle, question: question ? questionFixture : null }),
  });
  initClient(store);
  const page = conceptPageSchema.parse({
    ...structuredClone(summaryFixture),
    implemented_by: [],
    feature: "demo",
    ...overrides,
  });
  render(
    <Provider store={store}>
      <MemoryRouter>
        <PageHeading page={page} />
      </MemoryRouter>
    </Provider>,
  );
}

const METAPLAN = {
  key: "demo/metaplan/index",
  title: "Roadmap",
  path: "doc/plans/demo/index.md",
};
const current = { state: "current", out_of_date: null } as const;

it("draws a metaplan's summary freshness as the Summary row, with no status chip beside it", () => {
  show({ ...METAPLAN, state: current });

  const row = screen.getByRole("region", { name: "Summary" });
  expect(within(row).getByText("Summary up to date")).toBeTruthy();
  expect(screen.queryByText("Summary current")).toBeNull();
  expect(screen.queryByRole("list", { name: "Status" })).toBeNull();
});

it("keeps a per-page Rebuild: secondary while the summary is out of date, quiet while it is current", () => {
  show({
    ...METAPLAN,
    state: { state: "stale", out_of_date: { reason: "source_changed", detail: null } },
  });
  const row = screen.getByRole("region", { name: "Summary" });
  expect(within(row).getByText("Summary out of date")).toBeTruthy();
  expect(within(row).getByText("Out of date: the source changed")).toBeTruthy();
  expect(within(row).getByRole("button", { name: "Rebuild" }).getAttribute("data-look")).toBe(
    "secondary",
  );
  cleanup();

  show({ ...METAPLAN, state: current });
  expect(
    within(screen.getByRole("region", { name: "Summary" }))
      .getByRole("button", { name: "Rebuild" })
      .getAttribute("data-look"),
  ).toBe("quiet");
});

it("states a waiting rebuild question as the row's count with Rebuild all and Not now, never beside 'current'", () => {
  show({ ...METAPLAN, state: current }, { question: true });

  const row = screen.getByRole("region", { name: "Summary" });
  expect(within(row).getByText("6 summaries out of date")).toBeTruthy();
  expect(within(row).getByText(/waiting for approval/)).toBeTruthy();
  expect(within(row).getByRole("button", { name: "Rebuild all" })).toBeTruthy();
  expect(within(row).getByRole("button", { name: "Not now" })).toBeTruthy();
  expect(screen.queryByText(/Summary current|Summary up to date/)).toBeNull();
  expect(screen.queryByRole("list", { name: "Status" })).toBeNull();
});

it("shows a browser that cannot author the words alone", () => {
  show({ ...METAPLAN, state: current }, { question: true, author: false });
  const row = screen.getByRole("region", { name: "Summary" });
  expect(within(row).getByText(/waiting for approval/)).toBeTruthy();
  expect(within(row).queryByRole("button")).toBeNull();
});

it("draws the vision's summary freshness as the Summary row too", () => {
  show({ key: "demo/product/vision", path: "doc/product/demo/index.md", state: current });
  expect(
    within(screen.getByRole("region", { name: "Summary" })).getByText("Summary up to date"),
  ).toBeTruthy();
});

it("keeps the status chip on a section's summary page", () => {
  show({ state: current });
  expect(screen.queryByRole("region", { name: "Summary" })).toBeNull();
  expect(screen.getByText("Summary current")).toBeTruthy();
});
