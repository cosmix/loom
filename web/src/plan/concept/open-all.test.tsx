import { act, renderHook } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it } from "vitest";

import type { ConceptPage } from "@/plan/api/schema-concept";

import { hasOpenable, useAllOpen, useFoldAll, useOpenAll } from "./open-all";
import { cite, pageOf, S1, S2, sent, src } from "./reading-column-fixtures";
import { notesAtom, openRunsAtom } from "./view-state";

type Store = ReturnType<typeof createStore>;

const SUMMARY = "demo/product/section_01";
const OVERVIEW = "overview/demo/product";

function wrapperOf(store: Store) {
  return ({ children }: { children: ReactNode }) => <Provider store={store}>{children}</Provider>;
}

/** Calls Open all once over `page`. */
function openAllOn(page: ConceptPage, seed?: (s: Store) => void) {
  const store = createStore();
  seed?.(store);
  const { result } = renderHook(() => useOpenAll(page), { wrapper: wrapperOf(store) });
  act(() => result.current());
  return store;
}

describe("useOpenAll", () => {
  beforeEach(() => window.sessionStorage.clear());

  it("opens every openable sentence of a summary page once and leaves plain ones closed", () => {
    const page = pageOf(SUMMARY, [
      sent("a", "Opens to intro.", [src(S1, "intro")]),
      sent("b", "Plain claim."),
      sent("c", "Opens to detail.", [src(S2, "detail")]),
    ]);
    const store = openAllOn(page, (seeded) => {
      const intro = [{ path: S1, anchor: "intro" }];
      seeded.set(openRunsAtom(SUMMARY), [
        { level: 1, sentenceId: "a", restsOn: intro, parent: null },
      ]);
      seeded.set(notesAtom(SUMMARY), [{ paragraphOf: "", text: "Section gone", path: S1 }]);
    });

    const open = store.get(openRunsAtom(SUMMARY));
    expect(open.map((run) => [run.sentenceId, run.level, run.parent])).toEqual([
      ["a", 1, null],
      ["c", 1, null],
    ]);
    expect(open[1]!.restsOn).toEqual([{ path: S2, anchor: "detail" }]);
    expect(store.get(notesAtom(SUMMARY))).toEqual([]);
  });

  it("opens an overview's sentences as level 0 runs", () => {
    const page = pageOf(OVERVIEW, [
      sent("o", "Overview opens.", [cite("section_01", "x")]),
      sent("p", "Plain overview line."),
    ]);
    const store = openAllOn(page);

    expect(store.get(openRunsAtom(OVERVIEW))).toEqual([
      { level: 0, sentenceId: "o", restsOn: [], parent: null },
    ]);
  });

  it("leaves the open runs alone when nothing is closed", () => {
    const page = pageOf(SUMMARY, [sent("b", "Plain claim.")]);
    const store = openAllOn(page);
    expect(store.get(openRunsAtom(SUMMARY))).toEqual([]);
    expect(hasOpenable(page)).toBe(false);
  });
});

describe("useAllOpen and useFoldAll", () => {
  beforeEach(() => window.sessionStorage.clear());

  const page = pageOf(OVERVIEW, [
    sent("o", "Overview opens.", [cite("section_01", "x")]),
    sent("p", "Plain overview line."),
    sent("q", "Overview opens too.", [cite("section_02", "y")]),
  ]);

  function hooksOn(store: Store) {
    return renderHook(
      () => ({ allOpen: useAllOpen(page), open: useOpenAll(page), fold: useFoldAll(page) }),
      { wrapper: wrapperOf(store) },
    );
  }

  it("reports all open only once every openable sentence is open, by hand or by Open all", () => {
    const store = createStore();
    const { result } = hooksOn(store);
    expect(result.current.allOpen).toBe(false);

    act(() =>
      store.set(openRunsAtom(OVERVIEW), [{ level: 0, sentenceId: "o", restsOn: [], parent: null }]),
    );
    expect(result.current.allOpen).toBe(false);

    act(() => result.current.open());
    expect(result.current.allOpen).toBe(true);
  });

  it("folds every run at every level, nested member runs included", () => {
    const store = createStore();
    const { result } = hooksOn(store);
    act(() => result.current.open());
    act(() =>
      store.set(openRunsAtom(OVERVIEW), (open) => [
        ...open,
        { level: 1, sentenceId: "m1", restsOn: [{ path: S1, anchor: "intro" }], parent: "o" },
      ]),
    );

    act(() => result.current.fold());
    expect(store.get(openRunsAtom(OVERVIEW))).toEqual([]);
    expect(result.current.allOpen).toBe(false);
  });

  it("never reports all open for a page with no openable sentence", () => {
    const plain = pageOf(SUMMARY, [sent("b", "Plain claim.")]);
    const { result } = renderHook(() => useAllOpen(plain), { wrapper: wrapperOf(createStore()) });
    expect(result.current).toBe(false);
  });
});
