import { createStore } from "jotai";
import { afterEach, describe, expect, it, vi } from "vitest";
import fixture from "@/plan/api/fixtures/files-doc-plan.json";
import { docViewSchema } from "@/plan/api/schema-files";
import { docViewAtom, passageOf, refreshAllDocViews, refreshDocView } from "./passages";

const doc = docViewSchema.parse({
  ...fixture,
  text: "Intro\n\n## Parent\nParent text\n### Child\nChild text\n## Next\nNext text",
  sections: [
    { level: 2, title: "Parent", anchor: "parent", start_line: 3, end_line: 6 },
    { level: 3, title: "Child", anchor: "child", start_line: 5, end_line: 6 },
    { level: 2, title: "Next", anchor: "next", start_line: 7, end_line: 8 },
  ],
});

afterEach(() => vi.unstubAllGlobals());

describe("passageOf", () => {
  it("cuts a nested section using its inclusive line range", () => {
    expect(passageOf(doc, "child")).toEqual({
      title: "Child",
      anchor: "child",
      startLine: 5,
      endLine: 6,
      markdown: "### Child\nChild text",
      hash: `${doc.hash}#child`,
    });
    expect(passageOf(doc, "parent")?.markdown).toBe(
      "## Parent\nParent text\n### Child\nChild text",
    );
  });

  it("cuts the preamble before the first heading", () => {
    expect(passageOf(doc, "")).toEqual({
      title: doc.title,
      anchor: "",
      startLine: 1,
      endLine: 2,
      markdown: "Intro\n",
      hash: `${doc.hash}#`,
    });
  });

  it("returns null for a missing anchor", () => {
    expect(passageOf(doc, "missing")).toBeNull();
  });
});

describe("document cache", () => {
  it("memoizes each path and refreshes one subscribed document", async () => {
    const store = createStore();
    const updated = { ...doc, hash: "updated" };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(JSON.stringify(doc)))
      .mockResolvedValueOnce(new Response(JSON.stringify(updated)));
    vi.stubGlobal("fetch", fetchMock);
    const view = docViewAtom("doc/product/a & b.md");
    expect(docViewAtom("doc/product/a & b.md")).toBe(view);
    const notify = vi.fn();
    const unsubscribe = store.sub(view, notify);
    expect(await store.get(view)).toEqual(doc);
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/doc?path=product%2Fa%20%26%20b.md",
      expect.anything(),
    );
    expect(await store.get(view)).toEqual(doc);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    refreshDocView(store, "doc/product/a & b.md");
    expect(await store.get(view)).toEqual(updated);
    expect(notify).toHaveBeenCalled();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    unsubscribe();
  });

  it("refreshes all loaded documents in the supplied store", async () => {
    const store = createStore();
    let version = "initial";
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        Promise.resolve(new Response(JSON.stringify({ ...doc, hash: version }))),
      );
    vi.stubGlobal("fetch", fetchMock);
    const first = docViewAtom("doc/product/refresh-all-one.md");
    const second = docViewAtom("doc/product/refresh-all-two.md");
    const unsubFirst = store.sub(first, () => {});
    const unsubSecond = store.sub(second, () => {});
    expect((await store.get(first)).hash).toBe("initial");
    expect((await store.get(second)).hash).toBe("initial");
    version = "new";
    refreshAllDocViews(store);
    expect((await store.get(first)).hash).toBe("new");
    expect((await store.get(second)).hash).toBe("new");
    expect(fetchMock).toHaveBeenCalledTimes(4);
    unsubFirst();
    unsubSecond();
  });
});
