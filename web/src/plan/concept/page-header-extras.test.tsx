import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import { authoringAtom, youAtom } from "@/plan/state/atoms";

import { PageHeaderExtras } from "./page-header-extras";

// The review marks fetch their own data; this test is about the Rebuild button's ids.
vi.mock("@/plan/review/built-marker", () => ({ BuiltMarker: () => null }));
vi.mock("@/plan/review/open-questions", () => ({ OpenQuestionCount: () => null }));

const ADA = { public_id: "0123456789abcdef", name: "Ada" };
const fetchMock = vi.fn<typeof fetch>();

function show(key: string) {
  const store = createStore();
  initClient(store);
  store.set(youAtom, ADA);
  store.set(authoringAtom, { holder: { ...ADA, away_since: null, frees_at: null } });
  const page = conceptPageSchema.parse({ ...structuredClone(summaryFixture), key });
  render(
    <Provider store={store}>
      <PageHeaderExtras page={page} />
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
