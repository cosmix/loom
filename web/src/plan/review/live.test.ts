import { createElement } from "react";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { afterEach, expect, it, vi } from "vitest";
import hello from "@/plan/api/fixtures/hello.json";
import server from "@/plan/api/fixtures/server-info.json";
import { connectPlanSocket } from "@/plan/api/socket";
import { planSocketAtom } from "@/plan/state/atoms";
import { hashesAtom, loadHashes, openQuestionsAtom, useReviewLive } from "./live";

class FakeSocket {
  static last: FakeSocket;
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  readyState = 0;
  constructor(_url: string) {
    FakeSocket.last = this;
  }
  open() {
    this.readyState = 1;
    this.onopen?.();
  }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
  send(_data: string) {}
  receive(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) });
  }
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function mount() {
  FakeSocket.last = undefined as unknown as FakeSocket;
  const replies: Record<string, unknown> = {
    "/api/server": server,
    "/api/open-questions": { counts: { "doc/a.md": 1 } },
    "/api/hashes": { head: "first", docs: { "doc/a.md": "a" } },
  };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => new Response(JSON.stringify(replies[url]))),
  );
  const store = createStore();
  const socket = connectPlanSocket(store, {
    WebSocketImpl: FakeSocket as unknown as typeof WebSocket,
  });
  store.set(planSocketAtom, socket);
  const hook = renderHook(useReviewLive, {
    wrapper: ({ children }) => createElement(Provider, { store }, children),
  });
  await waitFor(() => expect(store.get(hashesAtom)?.head).toBe("first"));
  await waitFor(() => expect(FakeSocket.last).toBeDefined());
  act(() => {
    FakeSocket.last.open();
    FakeSocket.last.receive(hello);
  });
  return { store, socket, hook, replies };
}

it("open_questions arrives as a review kind", async () => {
  const { store, socket, hook } = await mount();
  await waitFor(() => expect(store.get(openQuestionsAtom)).toEqual({ "doc/a.md": 1 }));
  act(() =>
    FakeSocket.last.receive({ type: "review", kind: "open_questions", counts: { "doc/b.md": 3 } }),
  );
  expect(store.get(openQuestionsAtom)).toEqual({ "doc/b.md": 3 });
  hook.unmount();
  socket.dispose();
});

it("doc_changed refreshes the hashes without a reconnect", async () => {
  const { store, socket, hook, replies } = await mount();
  replies["/api/hashes"] = { head: "first", docs: { "doc/a.md": "edited" } };
  act(() =>
    FakeSocket.last.receive({ type: "doc_changed", path: "doc/product/section_02.md", hash: "x" }),
  );
  await waitFor(() => expect(store.get(hashesAtom)?.docs).toEqual({ "doc/a.md": "edited" }));
  hook.unmount();
  socket.dispose();
});

it("a doc_changed burst costs one load in flight and one more after it", async () => {
  const { store, socket, hook, replies } = await mount();
  const fetchMock = vi.mocked(fetch);
  const loads = () => fetchMock.mock.calls.filter(([url]) => url === "/api/hashes").length;
  const before = loads();
  let release: () => void = () => {};
  const gate = new Promise<void>((resolve) => (release = resolve));
  fetchMock.mockImplementation(async (url) => {
    await gate;
    return new Response(JSON.stringify(replies[url as string]));
  });
  replies["/api/hashes"] = { head: "first", docs: { "doc/a.md": "last" } };
  act(() => {
    for (const path of ["doc/a.md", "doc/b.md", "doc/c.md"]) {
      FakeSocket.last.receive({ type: "doc_changed", path, hash: null });
    }
  });
  release();
  await waitFor(() => expect(store.get(hashesAtom)?.docs).toEqual({ "doc/a.md": "last" }));
  await waitFor(() => expect(loads() - before).toBe(2));
  hook.unmount();
  socket.dispose();
});

it("resync refetches counts and hashes", async () => {
  const { store, socket, hook, replies } = await mount();
  replies["/api/open-questions"] = { counts: { "doc/b.md": 2 } };
  replies["/api/hashes"] = { head: "second", docs: { "doc/b.md": "b" } };
  act(() => FakeSocket.last.receive(hello));
  await waitFor(() => expect(store.get(openQuestionsAtom)).toEqual({ "doc/b.md": 2 }));
  await waitFor(() => expect(store.get(hashesAtom)).toEqual(replies["/api/hashes"]));
  hook.unmount();
  socket.dispose();
});

it("loadHashes queues behind a load in flight and resolves with the response it applied", async () => {
  const bodies = [
    { head: "one", docs: {} },
    { head: "two", docs: {} },
  ];
  const fetchMock = vi.fn(async () => new Response(JSON.stringify(bodies.shift())));
  vi.stubGlobal("fetch", fetchMock);
  const store = createStore();

  const [first, second] = await Promise.all([loadHashes(store), loadHashes(store)]);

  // The second caller gets a response read after its call, not the one already in flight.
  expect(first.ok && first.value.head).toBe("one");
  expect(second.ok && second.value.head).toBe("two");
  expect(store.get(hashesAtom)?.head).toBe("two");
  expect(fetchMock).toHaveBeenCalledTimes(2);
});
