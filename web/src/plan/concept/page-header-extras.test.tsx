import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import questionFixture from "@/plan/api/fixtures/summary-question.json";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import { summaryPanelSchema } from "@/plan/api/schema-summaries";
import { panelLoadAtom } from "@/plan/summaries/state";
import { authoringAtom, youAtom } from "@/plan/state/atoms";

import { PageHeaderExtras, SummaryAction } from "./page-header-extras";

// The build marker fetches its own data; these tests are about the summary action and the extras.
vi.mock("@/plan/review/built-marker", () => ({ BuiltMarker: () => null }));

const ADA = { public_id: "0123456789abcdef", name: "Ada" };
const fetchMock = vi.fn<typeof fetch>();

function show(key: string, overrides: Record<string, unknown> = {}, pending = false) {
  const store = createStore();
  if (pending) {
    const panel = summaryPanelSchema.parse({ ...panelFixture, question: questionFixture });
    store.set(panelLoadAtom, { status: "ready", panel });
  }
  initClient(store);
  store.set(youAtom, ADA);
  store.set(authoringAtom, { holder: { ...ADA, away_since: null, frees_at: null } });
  const page = conceptPageSchema.parse({ ...structuredClone(summaryFixture), key, ...overrides });
  return render(
    <Provider store={store}>
      <SummaryAction page={page} />
      <div data-testid="extras">
        <PageHeaderExtras page={page} />
      </div>
    </Provider>,
  );
}

beforeEach(() => {
  fetchMock.mockReset();
  fetchMock.mockImplementation(
    async () =>
      new Response(JSON.stringify(panelFixture), {
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

it.each([
  ["demo/engineering/my-plan", ["plans/my-plan"]],
  ["demo/product/section_01", ["demo/product/section_01"]],
  ["overview/demo/product", ["overview/demo/product"]],
])("rebuilds the summary behind page %s", async (key, ids) => {
  show(key);
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Rebuild" }));
  });
  await waitFor(() => expect(fetchMock).toHaveBeenCalledOnce());
  expect(fetchMock).toHaveBeenCalledWith(
    "/api/summaries/rebuild",
    expect.objectContaining({ method: "POST", body: JSON.stringify({ ids }) }),
  );
});

it("leaves Rebuild out of the extras, which render nothing without plan actions", () => {
  show("overview/demo/product");
  expect(screen.getByTestId("extras").textContent).toBe("");
  expect(
    screen.getByTestId("extras").contains(screen.getByRole("button", { name: "Rebuild" })),
  ).toBe(false);
});

it("offers no summary action for a page without a source path", () => {
  show("demo/product/section_01", { path: null });
  expect(screen.queryByRole("button", { name: /Rebuild/ })).toBeNull();
});

it("offers Rebuild all with its one-line reason while an approval is pending", () => {
  show("demo/product/section_01", {}, true);
  const approval = screen.getByRole("group", { name: "Rebuild approval" });
  expect(approval.querySelector("button")?.textContent).toBe("Rebuild all");
  expect(approval.textContent).toMatch(/above the budget and waiting for approval/);
  expect(screen.queryByRole("button", { name: "Rebuild" })).toBeNull();
});
