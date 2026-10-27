import { createStore } from "jotai";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import {
  conceptPageSchema,
  type ConceptChanged,
  type ConceptPage,
  type OutOfDate,
} from "@/plan/api/schema-concept";

import { applyConceptChanged, conceptPageAtom, loadConceptPages } from "./page-state";

vi.mock("./passages", () => ({ refreshDocView: vi.fn(), refreshAllDocViews: vi.fn() }));

type Store = ReturnType<typeof createStore>;

const SOURCE_CHANGED: OutOfDate = { reason: "source_changed", detail: null };
let served: Record<string, unknown> = {};
/** The page keys fetched, in request order. */
let requests: string[] = [];
/** While set, a request reads its body now but answers only when the gate opens. */
let gate: Promise<void> | null = null;

/** A page of one scope: the fixture's three-chip key row with exactly `dashed` ids out of date. */
function pageOf(key: string, dashed: string[]): ConceptPage {
  const page = conceptPageSchema.parse({ ...structuredClone(overviewFixture), key });
  const key_chips = page.key_chips.map((chip) => ({
    ...chip,
    out_of_date: dashed.includes(chip.doc.id) ? { ...SOURCE_CHANGED } : null,
  }));
  return { ...page, key_chips };
}

function chipState(store: Store, key: string, id: string): OutOfDate | null | undefined {
  const page = store.get(conceptPageAtom(key)).page;
  return page?.key_chips.find((chip) => chip.doc.id === id)?.out_of_date;
}

function loaded(pages: ConceptPage[]): Store {
  const store = createStore();
  for (const page of pages) {
    store.set(conceptPageAtom(page.key), { page, previous: null, error: null });
  }
  return store;
}

/** Whether a fetched page replaced the one before it (a dash alone sets `previous` to the page). */
function replaced(store: Store, key: string): boolean {
  const state = store.get(conceptPageAtom(key));
  return state.previous !== null && state.previous !== state.page;
}

async function deliver(store: Store, changed: Partial<ConceptChanged>): Promise<void> {
  applyConceptChanged(store, { pages: [], out_of_date: [], sources: [], ...changed });
  await vi.waitFor(() => expect(requests.length).toBeGreaterThan(0));
}

beforeEach(() => {
  served = {};
  requests = [];
  gate = null;
  vi.stubGlobal("fetch", async (input: RequestInfo | URL) => {
    const url = new URL(String(input), "http://localhost");
    const key = decodeURIComponent(url.pathname.slice("/api/concept/".length));
    requests.push(key);
    const body = served[key];
    await gate;
    if (!body) return new Response(JSON.stringify({ error: "not_found" }), { status: 404 });
    return new Response(JSON.stringify(body), { headers: { "content-type": "application/json" } });
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("a delta that names an overview refetches the loaded pages of its scope", () => {
  it("refetches a sibling summary page the delta does not name", async () => {
    const store = loaded([
      pageOf("product", ["section_01"]),
      pageOf("product/section_01", ["section_01"]),
      pageOf("product/section_02", ["section_01"]),
    ]);
    served.product = pageOf("product", []);
    served["product/section_01"] = pageOf("product/section_01", []);
    served["product/section_02"] = pageOf("product/section_02", []);

    await deliver(store, { pages: ["product"] });
    await vi.waitFor(() => expect(replaced(store, "product/section_02")).toBe(true));

    expect(chipState(store, "product/section_02", "section_01")).toBeNull();
    expect(chipState(store, "product/section_01", "section_01")).toBeNull();
    expect(chipState(store, "product", "section_01")).toBeNull();
  });

  it("does not refetch a loaded page of the other scope", async () => {
    const other = pageOf("engineering/plan_a", ["section_01"]);
    const store = loaded([pageOf("product", []), other]);
    const before = store.get(conceptPageAtom("engineering/plan_a"));
    served.product = pageOf("product", ["section_01"]);
    served["engineering/plan_a"] = pageOf("engineering/plan_a", []);

    await deliver(store, { pages: ["product"] });
    await vi.waitFor(() => expect(replaced(store, "product")).toBe(true));

    expect(requests).toEqual(["product"]);
    expect(store.get(conceptPageAtom("engineering/plan_a"))).toBe(before);
  });

  it("fetches no page twice when the delta also names a sibling", async () => {
    const store = loaded([
      pageOf("product", []),
      pageOf("product/section_01", []),
      pageOf("product/section_02", []),
    ]);
    for (const key of ["product", "product/section_01", "product/section_02"]) {
      served[key] = pageOf(key, []);
    }

    await deliver(store, { pages: ["product", "product/section_01", "product/section_02"] });
    await vi.waitFor(() => expect(replaced(store, "product/section_02")).toBe(true));

    expect([...requests].sort()).toEqual(["product", "product/section_01", "product/section_02"]);
  });

  it("never fetches a page of the scope that is not loaded", async () => {
    const store = loaded([pageOf("product", [])]);
    served.product = pageOf("product", []);
    served["product/section_02"] = pageOf("product/section_02", []);

    await deliver(store, { pages: ["product"] });
    await vi.waitFor(() => expect(replaced(store, "product")).toBe(true));

    expect(requests).toEqual(["product"]);
  });

  it("leaves the siblings alone when the delta names only a summary page", async () => {
    const store = loaded([
      pageOf("product", []),
      pageOf("product/section_01", []),
      pageOf("product/section_02", []),
    ]);
    served["product/section_01"] = pageOf("product/section_01", []);

    await deliver(store, { pages: ["product/section_01"] });
    await vi.waitFor(() => expect(replaced(store, "product/section_01")).toBe(true));

    expect(requests).toEqual(["product/section_01"]);
  });
});

describe("responses are ordered per page, never across pages", () => {
  it("ends with the later dash on every page when an older response arrives last", async () => {
    const store = loaded([
      pageOf("product", []),
      pageOf("product/section_01", []),
      pageOf("product/section_02", []),
    ]);
    // Delta 1: section_01 was rebuilt. Its page is served before section_02 goes stale.
    let open = () => {};
    gate = new Promise<void>((resolve) => {
      open = () => resolve();
    });
    served["product/section_01"] = pageOf("product/section_01", []);
    const first = loadConceptPages(store, ["product/section_01"]);
    await vi.waitFor(() => expect(requests).toEqual(["product/section_01"]));
    gate = null;

    // Delta 2: section_02 went stale and the overview is named.
    for (const key of ["product", "product/section_01", "product/section_02"]) {
      served[key] = pageOf(key, ["section_02"]);
    }
    applyConceptChanged(store, { pages: ["product"], out_of_date: ["section_02"], sources: [] });
    await vi.waitFor(() => {
      for (const key of ["product", "product/section_01", "product/section_02"]) {
        expect(replaced(store, key)).toBe(true);
      }
    });

    open();
    await first;

    for (const key of ["product", "product/section_01", "product/section_02"]) {
      expect(chipState(store, key, "section_02")).toEqual(SOURCE_CHANGED);
    }
  });
});
