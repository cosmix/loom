import { act, renderHook, waitFor } from "@testing-library/react";
import { createStore } from "jotai/vanilla";
import { afterEach, describe, expect, it, vi } from "vitest";
import docFixture from "@/plan/api/fixtures/files-doc-plan.json";
import saved from "@/plan/api/fixtures/files-save-saved.json";
import docChanged from "@/plan/api/fixtures/files-doc-changed.json";
import conflict from "@/plan/api/fixtures/files-save-conflict.json";
import invalid from "@/plan/api/fixtures/files-save-invalid.json";
import treeChanged from "@/plan/api/fixtures/files-tree-changed.json";
import tree from "@/plan/api/fixtures/files-tree.json";
import validationFixture from "@/plan/api/fixtures/files-validation.json";
import { docViewSchema, fileTreeSchema } from "@/plan/api/schema-files";
import {
  fetchDoc,
  fetchHeadDiff,
  fetchTree,
  fetchTurnDiff,
  saveDoc,
  searchDocs,
  useFileSocket,
} from "@/plan/files/api";
import {
  docChangedAtom,
  openDocAtom,
  openDocIsStaleAtom,
  treeAtom,
  validationAtom,
} from "@/plan/files/atoms";
import { planSocketAtom } from "@/plan/state/atoms";

afterEach(() => vi.unstubAllGlobals());

describe("file API", () => {
  it("returns save outcomes for 200, 409 and 422 and reports other errors", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(JSON.stringify(saved), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(conflict), { status: 409 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(invalid), { status: 422 }))
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ error: "forbidden", message: "No access" }), { status: 403 }),
      );
    vi.stubGlobal("fetch", fetchMock);
    for (const value of [saved, conflict, invalid]) {
      expect(await saveDoc("a & b.md", "text", "sha256:x+y", true)).toEqual({ ok: true, value });
    }
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/save?path=a%20%26%20b.md&hash=sha256%3Ax%2By&with_errors=1",
      {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "text/plain; charset=utf-8" },
        body: "text",
      },
    );
    expect(await saveDoc("a", "text", "hash", false)).toEqual({
      ok: false,
      status: 403,
      error: "forbidden",
      message: "No access",
    });
  });

  it("encodes query values for each read endpoint", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response("{}"));
    vi.stubGlobal("fetch", fetchMock);
    await fetchTree();
    await fetchDoc("a & b");
    await searchDocs("x & y", "all");
    await fetchHeadDiff("a/b");
    await fetchTurnDiff("t/1", "a/b");
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      "/api/tree",
      "/api/doc?path=a%20%26%20b",
      "/api/search?q=x%20%26%20y&scope=all",
      "/api/diff/head?path=a%2Fb",
      "/api/diff/turn?turn=t%2F1&path=a%2Fb",
    ]);
  });

  it("refetches on resync and marks a missed document change stale", async () => {
    const store = createStore();
    const doc = docViewSchema.parse(docFixture);
    store.set(openDocAtom, { doc, seenHash: doc.hash });
    const handlers: Record<string, (value: never) => void> = {};
    let resync = () => {};
    const unsubscribe = vi.fn();
    store.set(planSocketAtom, {
      subscribe: (type: string, handler: (value: never) => void) => {
        handlers[type] = handler;
        return unsubscribe;
      },
      onResync: (handler: () => void) => {
        resync = handler;
        return unsubscribe;
      },
    } as never);
    const updated = { ...doc, hash: "sha256:new" };
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockImplementation((url: string) =>
          Promise.resolve(new Response(JSON.stringify(url === "/api/tree" ? tree : updated))),
        ),
    );
    const hook = renderHook(() => useFileSocket(store));
    act(() => resync());
    await waitFor(() => expect(store.get(openDocIsStaleAtom)).toBe(true));
    expect(store.get(docChangedAtom)[doc.path]).toBe(updated.hash);
    expect(store.get(treeAtom)).toEqual(tree);
    expect(store.get(validationAtom)[doc.path]).toEqual(doc.plan?.validation);
    hook.unmount();
    expect(unsubscribe).toHaveBeenCalledTimes(4);
  });
});

describe("file API paths", () => {
  it("sends repo-relative paths as doc-relative request parameters", async () => {
    const fetchMock = vi.fn().mockImplementation(() => Promise.resolve(new Response("{}")));
    vi.stubGlobal("fetch", fetchMock);
    await fetchDoc("doc/product/a.md");
    await fetchHeadDiff("doc/product/a.md");
    await fetchTurnDiff("t1", "doc/product/a.md");
    await saveDoc("doc/product/a.md", "text", "sha256:x", false);
    await saveDoc("doc/product/a.md", "text", "sha256:x", true);
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      "/api/doc?path=product%2Fa.md",
      "/api/diff/head?path=product%2Fa.md",
      "/api/diff/turn?turn=t1&path=product%2Fa.md",
      "/api/save?path=product%2Fa.md&hash=sha256%3Ax",
      "/api/save?path=product%2Fa.md&hash=sha256%3Ax&with_errors=1",
    ]);
  });
});

type Handler = (value: never) => void;

function fakeSocket() {
  const handlers: Record<string, Handler> = {};
  const resyncs: (() => void)[] = [];
  const unsubscribe = vi.fn();
  const socket = {
    subscribe: (type: string, handler: Handler) => {
      handlers[type] = handler;
      return unsubscribe;
    },
    onResync: (handler: () => void) => {
      resyncs.push(handler);
      return unsubscribe;
    },
  };
  return {
    socket: socket as never,
    emit: (type: string, message: object) => act(() => handlers[type]?.(message as never)),
    resync: () => act(() => resyncs.forEach((handler) => handler())),
  };
}

const flush = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));

describe("file socket", () => {
  it("writes pushed trees, document changes and validation into their atoms", () => {
    const store = createStore();
    const { socket, emit } = fakeSocket();
    store.set(planSocketAtom, socket);
    vi.stubGlobal("fetch", vi.fn().mockReturnValue(new Promise(() => {})));
    const hook = renderHook(() => useFileSocket(store));
    emit("tree_changed", treeChanged);
    const { type: _type, ...pushedTree } = treeChanged;
    expect(store.get(treeAtom)).toEqual(pushedTree);
    emit("doc_changed", { type: "doc_changed", ...docChanged });
    expect(store.get(docChangedAtom)).toEqual({ [docChanged.path]: null });
    emit("doc_changed", { type: "doc_changed", path: docChanged.path, hash: "sha256:next" });
    expect(store.get(docChangedAtom)).toEqual({ [docChanged.path]: "sha256:next" });
    emit("validation", { type: "validation", ...validationFixture });
    expect(store.get(validationAtom)).toEqual({
      [validationFixture.path]: validationFixture.validation,
    });
    hook.unmount();
  });

  it("starts each mount from a fresh tree and no remembered changes", async () => {
    const store = createStore();
    const fetchMock = vi
      .fn()
      .mockImplementation(() => Promise.resolve(new Response(JSON.stringify(tree))));
    vi.stubGlobal("fetch", fetchMock);
    const parsed = fileTreeSchema.parse(tree);
    for (const visit of [1, 2]) {
      store.set(treeAtom, { ...parsed, product: [] });
      store.set(docChangedAtom, { "doc/product/a.md": "sha256:old" });
      store.set(validationAtom, { "doc/plans/p.md": validationFixture.validation as never });
      const hook = renderHook(() => useFileSocket(store));
      expect(store.get(docChangedAtom)).toEqual({});
      expect(store.get(validationAtom)).toEqual({});
      await waitFor(() => expect(store.get(treeAtom)).toEqual(parsed));
      expect(fetchMock).toHaveBeenCalledTimes(visit);
      hook.unmount();
    }
  });

  it("keeps a pushed tree over a refetch that was already in flight", async () => {
    const store = createStore();
    const { socket, emit } = fakeSocket();
    store.set(planSocketAtom, socket);
    let answer: (response: Response) => void = () => {};
    vi.stubGlobal(
      "fetch",
      vi.fn().mockReturnValue(new Promise<Response>((resolve) => (answer = resolve))),
    );
    renderHook(() => useFileSocket(store));
    emit("tree_changed", treeChanged);
    const { type: _type, ...pushedTree } = treeChanged;
    answer(new Response(JSON.stringify(tree)));
    await flush();
    expect(store.get(treeAtom)).toEqual(pushedTree);
  });

  it("marks the open document changed to nothing when a resync finds it gone", async () => {
    const store = createStore();
    const doc = docViewSchema.parse(docFixture);
    store.set(openDocAtom, { doc, seenHash: doc.hash });
    const { socket, resync } = fakeSocket();
    store.set(planSocketAtom, socket);
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockImplementation((url: string) =>
          Promise.resolve(
            url === "/api/tree"
              ? new Response(JSON.stringify(tree))
              : new Response(JSON.stringify({ error: "not_found" }), { status: 404 }),
          ),
        ),
    );
    renderHook(() => useFileSocket(store));
    resync();
    await waitFor(() => expect(store.get(docChangedAtom)[doc.path]).toBeNull());
    expect(store.get(openDocIsStaleAtom)).toBe(true);
  });
});
