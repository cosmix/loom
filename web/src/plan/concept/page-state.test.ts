import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createElement } from "react";
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";

import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import {
  conceptPageSchema,
  type ConceptChanged,
  type ConceptPage,
} from "@/plan/api/schema-concept";
import { planSocketAtom } from "@/plan/state/atoms";

import { pageSentences } from "./open-state";
import {
  conceptPageAtom,
  isMissing,
  loadConceptPage,
  subscribeConcept,
  useConceptPage,
  useConceptSubscription,
} from "./page-state";
import { refreshAllDocViews, refreshDocView } from "./passages";
import { notesAtom, openRunsAtom } from "./view-state";

vi.mock("./passages", () => ({ refreshDocView: vi.fn(), refreshAllDocViews: vi.fn() }));

type Store = ReturnType<typeof createStore>;

const SECTION_01 = "doc/product/section_01.md";
let served: Record<string, unknown> = {};
let held: Record<string, Promise<void>> = {};
type ConceptMessage = ConceptChanged & { type: "concept_changed" };

let fetchMock: Mock<typeof stubFetch>;
const changedHandlers = new Set<(message: ConceptMessage) => void>();
const resyncHandlers = new Set<() => void>();

function overview(): ConceptPage {
  return conceptPageSchema.parse({ ...structuredClone(overviewFixture), key: "product" });
}

function summary(): ConceptPage {
  return conceptPageSchema.parse(structuredClone(summaryFixture));
}

function withFirstText(page: ConceptPage, text: string): ConceptPage {
  const copy = structuredClone(page);
  pageSentences(copy)[0]!.text = text;
  return copy;
}

async function stubFetch(input: RequestInfo | URL): Promise<Response> {
  const url = new URL(String(input), "http://localhost");
  const key = decodeURIComponent(url.pathname.slice("/api/concept/".length));
  await held[key];
  const body = served[key];
  if (!body) return new Response(JSON.stringify({ error: "not_found" }), { status: 404 });
  return new Response(JSON.stringify(body), { headers: { "content-type": "application/json" } });
}

function fetchedKeys(): string[] {
  return fetchMock.mock.calls.map(([input]) => new URL(String(input), "http://localhost").pathname);
}

function fakeSocket() {
  return {
    subscribe: (_type: string, handler: (message: ConceptMessage) => void) => {
      changedHandlers.add(handler);
      return () => changedHandlers.delete(handler);
    },
    onResync: (handler: () => void) => {
      resyncHandlers.add(handler);
      return () => resyncHandlers.delete(handler);
    },
  };
}

function deliver(changed: Partial<ConceptChanged>): void {
  const message: ConceptMessage = {
    type: "concept_changed",
    pages: [],
    out_of_date: [],
    sources: [],
    ...changed,
  };
  act(() => changedHandlers.forEach((handler) => handler(message)));
}

function connected(): Store {
  const store = createStore();
  store.set(planSocketAtom, fakeSocket() as never);
  return store;
}

function hold(key: string): () => void {
  let release = () => {};
  held[key] = new Promise<void>((resolve) => {
    release = resolve;
  });
  return release;
}

beforeEach(() => {
  served = { product: overview(), "product/section_01": summary() };
  held = {};
  changedHandlers.clear();
  resyncHandlers.clear();
  window.sessionStorage.clear();
  fetchMock = vi.fn(stubFetch);
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

describe("concept_changed", () => {
  it("concept_changed out_of_date dashes marks before the refetch", async () => {
    const store = connected();
    await loadConceptPage(store, "product");
    const stop = subscribeConcept(store);
    const release = hold("product");
    deliver({ pages: ["product"], out_of_date: ["section_02"] });
    const { page } = store.get(conceptPageAtom("product"));
    const chip = page!.key_chips.find((entry) => entry.doc.id === "section_02");
    expect(chip?.out_of_date).toEqual({ reason: "source_changed", detail: null });
    const marks = pageSentences(page!)[0]!.marks;
    expect(marks.find((mark) => mark.doc.id === "section_02")?.out_of_date?.reason).toBe(
      "source_changed",
    );
    const before = pageSentences(overview())[0]!.marks;
    expect(marks.find((mark) => mark.doc.id === "section_01")).toEqual(
      before.find((mark) => mark.doc.id === "section_01"),
    );
    expect(pageSentences(page!)[0]!.text).toBe(pageSentences(overview())[0]!.text);
    release();
    await waitFor(() => expect(store.get(conceptPageAtom("product")).page).toEqual(overview()));
    stop();
  });

  it("listed loaded page refetches and keeps previous", async () => {
    const store = connected();
    await loadConceptPage(store, "product");
    const before = store.get(conceptPageAtom("product")).page;
    const stop = subscribeConcept(store);
    served.product = withFirstText(overview(), "Rebuilt first sentence.");
    deliver({ pages: ["product"], sources: [SECTION_01] });
    await waitFor(() => expect(store.get(conceptPageAtom("product")).previous).toBe(before));
    const { page } = store.get(conceptPageAtom("product"));
    expect(pageSentences(page!)[0]!.text).toBe("Rebuilt first sentence.");
    stop();
  });

  it("sources refetch open passages", () => {
    const store = connected();
    const stop = subscribeConcept(store);
    deliver({ sources: [SECTION_01, "doc/product/vision.md"] });
    expect(vi.mocked(refreshDocView).mock.calls).toEqual([
      [store, SECTION_01],
      [store, "doc/product/vision.md"],
    ]);
    stop();
  });

  it("unlisted page is not fetched", async () => {
    const store = connected();
    await loadConceptPage(store, "product");
    const stop = subscribeConcept(store);
    deliver({ pages: ["product/section_01", "engineering"] });
    expect(fetchedKeys()).toEqual(["/api/concept/product"]);
    stop();
  });

  it("concept_changed without sources refetches the named pages", async () => {
    const store = connected();
    await loadConceptPage(store, "product");
    await loadConceptPage(store, "product/section_01");
    const stop = subscribeConcept(store);
    fetchMock.mockClear();
    deliver({ pages: ["product", "product/section_01"] });
    await waitFor(() =>
      expect(fetchedKeys()).toEqual(["/api/concept/product", "/api/concept/product/section_01"]),
    );
    expect(refreshDocView).not.toHaveBeenCalled();
    stop();
  });

  it("subscribes once the plan socket appears", async () => {
    const store = createStore();
    await loadConceptPage(store, "product");
    const stop = subscribeConcept(store);
    expect(changedHandlers.size).toBe(0);
    store.set(planSocketAtom, fakeSocket() as never);
    expect(changedHandlers.size).toBe(1);
    stop();
    expect(changedHandlers.size).toBe(0);
    expect(resyncHandlers.size).toBe(0);
  });
});

describe("overtaken requests", () => {
  it("an overtaken response is dropped", async () => {
    const key = "product/section_01";
    const store = createStore();
    const release = hold(key);
    const first = loadConceptPage(store, key);
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    delete held[key];
    served[key] = withFirstText(summary(), "Version two.");
    await loadConceptPage(store, key);
    expect(pageSentences(store.get(conceptPageAtom(key)).page!)[0]!.text).toBe("Version two.");
    served[key] = withFirstText(summary(), "Version one.");
    release();
    await first;
    expect(pageSentences(store.get(conceptPageAtom(key)).page!)[0]!.text).toBe("Version two.");
  });
});

describe("resync and remount", () => {
  it("resync refetches every loaded page", async () => {
    const store = connected();
    await loadConceptPage(store, "product");
    await loadConceptPage(store, "product/section_01");
    const stop = subscribeConcept(store);
    fetchMock.mockClear();
    act(() => resyncHandlers.forEach((handler) => handler()));
    await waitFor(() =>
      expect(fetchedKeys().sort()).toEqual([
        "/api/concept/product",
        "/api/concept/product/section_01",
      ]),
    );
    expect(refreshAllDocViews).toHaveBeenCalledWith(store);
    stop();
  });

  it("returning to the tab after a concept_changed shows the new page", async () => {
    function Page() {
      useConceptSubscription();
      const { page } = useConceptPage("product");
      return createElement("p", null, page ? pageSentences(page)[0]!.text : "loading");
    }
    const store = connected();
    // A .ts test file has no JSX: build the tree with createElement.
    const view = () => render(createElement(Provider, { store }, createElement(Page)));
    const first = view();
    const oldText = pageSentences(overview())[0]!.text;
    expect(await screen.findByText(oldText)).toBeTruthy();
    first.unmount();
    expect(changedHandlers.size).toBe(0);
    served.product = withFirstText(overview(), "Rebuilt while away.");
    deliver({ pages: ["product"] });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const release = hold("product");
    view();
    expect(screen.getByText(oldText)).toBeTruthy();
    release();
    expect(await screen.findByText("Rebuilt while away.")).toBeTruthy();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(pageSentences(store.get(conceptPageAtom("product")).previous!)[0]!.text).toBe(oldText);
  });

  it("reload after a rebuild moves the stored run by overlap and writes the fold note", async () => {
    const key = "product/section_01";
    const stored = [
      {
        level: 1,
        sentenceId: "old-1",
        restsOn: [{ path: SECTION_01, anchor: "stages" }],
        parent: null,
      },
      {
        level: 1,
        sentenceId: "old-2",
        restsOn: [{ path: SECTION_01, anchor: "gone", title: "Gone" }],
        parent: null,
      },
    ];
    window.sessionStorage.setItem(`loom-plan:concept:${key}`, JSON.stringify(stored));
    const store = createStore();
    await loadConceptPage(store, key);
    expect(store.get(openRunsAtom(key))).toEqual([
      {
        level: 1,
        sentenceId: "s2",
        restsOn: [{ path: SECTION_01, anchor: "stages" }],
        parent: null,
      },
    ]);
    expect(store.get(notesAtom(key))).toEqual([
      {
        paragraphOf: "",
        text: "This summary was rewritten and no longer draws on Gone",
        path: SECTION_01,
      },
    ]);
    expect(
      JSON.parse(window.sessionStorage.getItem(`loom-plan:concept:${key}`) ?? "[]"),
    ).toHaveLength(1);
  });
});

describe("pages the server does not have", () => {
  const key = "product/section_01";
  const failWith = (status: number, error: string, message: string) =>
    fetchMock.mockImplementationOnce(
      async () => new Response(JSON.stringify({ error, message }), { status }),
    );
  it("a refetch that returns 404 drops the loaded page", async () => {
    const store = createStore();
    await loadConceptPage(store, key);
    expect(store.get(conceptPageAtom(key)).page).not.toBeNull();
    delete served[key];
    await loadConceptPage(store, key);
    const state = store.get(conceptPageAtom(key));
    expect(state).toMatchObject({ page: null, previous: null, status: 404 });
    expect(isMissing(state)).toBe(true);
  });

  it("a 400 bad_page answer reads as missing too", async () => {
    const store = createStore();
    failWith(400, "bad_page", "not a concept page key");
    await loadConceptPage(store, key);
    const state = store.get(conceptPageAtom(key));
    expect(state).toMatchObject({ page: null, error: "not a concept page key", status: 400 });
    expect(isMissing(state)).toBe(true);
  });

  it("a refetch that fails with 500 keeps the page and is not missing", async () => {
    const store = createStore();
    await loadConceptPage(store, key);
    failWith(500, "internal", "database is down");
    await loadConceptPage(store, key);
    const state = store.get(conceptPageAtom(key));
    expect(state.page).not.toBeNull();
    expect(state).toMatchObject({ error: "database is down", status: 500 });
    expect(isMissing(state)).toBe(false);
    await loadConceptPage(store, key);
    expect(store.get(conceptPageAtom(key)).status).toBeUndefined();
  });

  it.each(["product/../../server", "product/..", "product/a/b", "plans/x", "product/", "..", ""])(
    "the key %j is never fetched",
    async (bad) => {
      const store = createStore();
      await loadConceptPage(store, bad);
      expect(fetchMock).not.toHaveBeenCalled();
      expect(store.get(conceptPageAtom(bad)).page).toBeNull();
    },
  );
});

describe("mounting after time away", () => {
  function Page({ pageKey }: { pageKey: string }) {
    useConceptSubscription();
    useConceptPage(pageKey);
    return null;
  }
  const mount = (store: Store, pageKey: string) =>
    createElement(Provider, { store }, createElement(Page, { pageKey }));

  it("refetches every cached page once and refreshes the passages", async () => {
    const store = connected();
    await loadConceptPage(store, "product");
    await loadConceptPage(store, "product/section_01");
    served["product/section_01"] = withFirstText(summary(), "Rewritten while away.");
    fetchMock.mockClear();
    render(mount(store, "product"));
    await waitFor(() =>
      expect(store.get(conceptPageAtom("product/section_01")).previous).not.toBeNull(),
    );
    expect(fetchedKeys().sort()).toEqual([
      "/api/concept/product",
      "/api/concept/product/section_01",
    ]);
    const cited = store.get(conceptPageAtom("product/section_01")).page!;
    expect(pageSentences(cited)[0]!.text).toBe("Rewritten while away.");
    expect(refreshAllDocViews).toHaveBeenCalledWith(store);
  });

  it("a key change while mounted refetches only that page", async () => {
    const store = connected();
    await loadConceptPage(store, "product");
    await loadConceptPage(store, "product/section_01");
    const view = render(mount(store, "product"));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(4));
    fetchMock.mockClear();
    vi.mocked(refreshAllDocViews).mockClear();
    view.rerender(mount(store, "product/section_01"));
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    expect(fetchedKeys()).toEqual(["/api/concept/product/section_01"]);
    expect(refreshAllDocViews).not.toHaveBeenCalled();
  });
});
