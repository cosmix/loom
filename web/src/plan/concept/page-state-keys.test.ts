import { cleanup, render } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createElement } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { conceptPageSchema } from "@/plan/api/schema-concept";

import { conceptPageAtom, useConceptPage, type ConceptPageState } from "./page-state";

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
  const page = conceptPageSchema.parse(structuredClone(summaryFixture));
  store.set(conceptPageAtom(key), { page, previous: null, error: null });
  const read: { state?: ConceptPageState } = {};
  function Page() {
    read.state = useConceptPage(key);
    return null;
  }
  render(createElement(Provider, { store }, createElement(Page)));
  return read.state!;
}

describe("useConceptPage with a key that is no page key", () => {
  it("never reads the page atom map, so a mistyped URL adds nothing to it", () => {
    const state = readWhileCached("product/../../server");
    expect(state).toEqual({ page: null, previous: null, error: null });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("a page key still reads its own atom", () => {
    expect(readWhileCached("product/section_01").page).not.toBeNull();
  });
});
