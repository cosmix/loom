import { act, renderHook } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it } from "vitest";

import { pageHref } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";

import { useOpenAll } from "./open-all";
import { conceptPageAtom } from "./page-state";
import { cite, pageOf, S1, S2, sent, src } from "./reading-column-fixtures";
import { notesAtom, openRunsAtom } from "./view-state";

type Store = ReturnType<typeof createStore>;

const SUMMARY = "demo/product/section_01";
const OVERVIEW = "overview/demo/product";

/** Calls Open all once at `path`, with `key`'s page loaded (or still loading when null). */
function openAllAt(path: string, key: string, page: ConceptPage | null, seed?: (s: Store) => void) {
  const store = createStore();
  store.set(conceptPageAtom(key), { page, previous: null, error: null });
  seed?.(store);
  const wrapper = ({ children }: { children: ReactNode }) => (
    <Provider store={store}>
      <MemoryRouter initialEntries={[path]}>{children}</MemoryRouter>
    </Provider>
  );
  const { result } = renderHook(() => useOpenAll(), { wrapper });
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
    const store = openAllAt(pageHref(SUMMARY), SUMMARY, page, (seeded) => {
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
    const store = openAllAt(pageHref(OVERVIEW), OVERVIEW, page);

    expect(store.get(openRunsAtom(OVERVIEW))).toEqual([
      { level: 0, sentenceId: "o", restsOn: [], parent: null },
    ]);
  });

  it("does nothing off a concept page or before the page loads", () => {
    const page = pageOf(SUMMARY, [sent("a", "Opens.", [src(S1, "intro")])]);
    const off = openAllAt("/", SUMMARY, page);
    expect(off.get(openRunsAtom(SUMMARY))).toEqual([]);

    const loading = openAllAt(pageHref(SUMMARY), SUMMARY, null);
    expect(loading.get(openRunsAtom(SUMMARY))).toEqual([]);
  });
});
