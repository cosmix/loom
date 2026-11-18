import { cleanup, render } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createElement } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { parsePageKey } from "@/plan/api/page-key";
import { conceptPageSchema } from "@/plan/api/schema-concept";

import {
  conceptPageAtom,
  loadConceptPage,
  useConceptPage,
  type ConceptPageState,
} from "./page-state";

vi.mock("./passages", () => ({ refreshDocView: vi.fn(), refreshAllDocViews: vi.fn() }));

const fetchMock = vi.fn(async () => new Response("{}", { status: 404 }));

beforeEach(() => {
  fetchMock.mockClear();
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/** The state `useConceptPage(key)` returns while `key` holds a loaded page in the atom map. */
function readWhileCached(key: string): ConceptPageState {
  const store = createStore();
  const feature = parsePageKey(key)?.kind === "standalone" ? null : "demo";
  const page = conceptPageSchema.parse({ ...structuredClone(summaryFixture), feature, key });
  store.set(conceptPageAtom(key), { page, previous: null, error: null });
  const read: { state?: ConceptPageState } = {};
  function Page() {
    read.state = useConceptPage(key);
    return null;
  }
  render(createElement(Provider, { store }, createElement(Page)));
  return read.state!;
}

describe("concept page keys", () => {
  it.each(["product", "product/x", "engineering", "demo/product/../../server"])(
    "ignores a cached page for the invalid key %j",
    (key) => {
      const state = readWhileCached(key);
      expect(state).toEqual({ page: null, previous: null, error: null });
      expect(fetchMock).not.toHaveBeenCalled();
    },
  );

  it.each(["overview/demo/product", "demo/product/section_01", "plans/standalone"])(
    "reads the cached page for the server key %s",
    (key) => {
      expect(readWhileCached(key).page?.key).toBe(key);
    },
  );

  it.each([
    "product",
    "product/x",
    "engineering",
    "demo/product/../../server",
    "demo/product/..",
    "demo/product/a/b",
    "plans/..",
    "demo/product/",
    "..",
    "",
  ])("never fetches the invalid key %j", async (key) => {
    const store = createStore();
    await loadConceptPage(store, key);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(store.get(conceptPageAtom(key)).page).toBeNull();
  });

  it("fetches a standalone plan through its server key", async () => {
    const key = "plans/standalone";
    const page = { ...summaryFixture, feature: null, key };
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify(page)));
    const store = createStore();

    await loadConceptPage(store, key);

    expect(fetchMock).toHaveBeenCalledWith("/api/concept/plans/standalone", expect.anything());
    expect(store.get(conceptPageAtom(key)).page).toEqual(page);
  });
});
