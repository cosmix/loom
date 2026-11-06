import { act, cleanup, render, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createElement } from "react";
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";

import traceFixture from "@/plan/api/fixtures/trace.json";
import { traceViewSchema, type TraceView } from "@/plan/api/schema-trace";
import { planSocketAtom } from "@/plan/state/atoms";

import { loadTrace, subscribeTrace, traceAtom, useTrace } from "./state";

type Store = ReturnType<typeof createStore>;
type ChangedMessage = { type: "trace_changed"; feature: string };

let served: Record<string, { status: number; body: unknown }> = {};
let held: Record<string, Promise<void>> = {};
let fetchMock: Mock<typeof stubFetch>;
const changedHandlers = new Set<(message: ChangedMessage) => void>();
const resyncHandlers = new Set<() => void>();

function demoTrace(title = "Demo"): TraceView {
  const view = traceViewSchema.parse(structuredClone(traceFixture));
  view.documents[0]!.title = title;
  return view;
}

function serve(feature: string, body: unknown, status = 200): void {
  served[feature] = { status, body };
}

async function stubFetch(input: RequestInfo | URL): Promise<Response> {
  const url = new URL(String(input), "http://localhost");
  const feature = decodeURIComponent(url.pathname.split("/").pop()!);
  // The answer is fixed when the request is made; a held request delivers it late.
  const answer = served[feature] ?? { status: 404, body: { error: "not_found" } };
  await held[feature];
  return new Response(JSON.stringify(answer.body), {
    status: answer.status,
    headers: { "content-type": "application/json" },
  });
}

function fakeSocket() {
  return {
    subscribe: (_type: string, handler: (message: ChangedMessage) => void) => {
      changedHandlers.add(handler);
      return () => changedHandlers.delete(handler);
    },
    onResync: (handler: () => void) => {
      resyncHandlers.add(handler);
      return () => resyncHandlers.delete(handler);
    },
  };
}

function connected(): Store {
  const store = createStore();
  store.set(planSocketAtom, fakeSocket() as never);
  return store;
}

function deliver(feature: string): void {
  act(() => changedHandlers.forEach((handler) => handler({ type: "trace_changed", feature })));
}

function hold(feature: string): () => void {
  let release = () => {};
  held[feature] = new Promise<void>((resolve) => {
    release = resolve;
  });
  return release;
}

function titleOf(store: Store, feature = "demo"): string | undefined {
  return store.get(traceAtom(feature)).trace?.documents[0]?.title;
}

beforeEach(() => {
  served = {};
  held = {};
  changedHandlers.clear();
  resyncHandlers.clear();
  fetchMock = vi.fn(stubFetch);
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("loadTrace", () => {
  it("stores the parsed trace", async () => {
    serve("demo", demoTrace());
    const store = connected();
    await loadTrace(store, "demo");
    expect(store.get(traceAtom("demo"))).toEqual({ trace: demoTrace(), error: null });
  });

  it("keeps the loaded view when a refetch fails, and records the error", async () => {
    serve("demo", demoTrace());
    const store = connected();
    await loadTrace(store, "demo");
    serve("demo", { error: "boom", message: "server down" }, 500);
    await loadTrace(store, "demo");
    const state = store.get(traceAtom("demo"));
    expect(state.trace).toEqual(demoTrace());
    expect(state.error).toBe("server down");
    expect(state.status).toBe(500);
  });

  it("drops the view on a 404", async () => {
    serve("demo", demoTrace());
    const store = connected();
    await loadTrace(store, "demo");
    serve("demo", { error: "not_found", message: "no such feature" }, 404);
    await loadTrace(store, "demo");
    expect(store.get(traceAtom("demo"))).toEqual({
      trace: null,
      error: "no such feature",
      status: 404,
    });
  });

  it("drops an answer overtaken by a later request", async () => {
    serve("demo", demoTrace("Old"));
    const store = connected();
    const release = hold("demo");
    const slow = loadTrace(store, "demo");
    serve("demo", demoTrace("New"));
    held.demo = Promise.resolve();
    await loadTrace(store, "demo");
    release();
    await slow;
    expect(titleOf(store)).toBe("New");
  });
});

describe("trace_changed", () => {
  it("refetches the named feature once it was requested", async () => {
    serve("demo", demoTrace("Before"));
    const store = connected();
    await loadTrace(store, "demo");
    const stop = subscribeTrace(store);
    serve("demo", demoTrace("After"));
    deliver("demo");
    await waitFor(() => expect(titleOf(store)).toBe("After"));
    stop();
  });

  it("ignores a feature nothing asked for", async () => {
    const store = connected();
    const stop = subscribeTrace(store);
    deliver("other");
    expect(fetchMock).not.toHaveBeenCalled();
    stop();
  });

  it("refetches every requested feature on a resync", async () => {
    serve("demo", demoTrace("Before"));
    serve("second", demoTrace("Second"));
    const store = connected();
    await loadTrace(store, "demo");
    await loadTrace(store, "second");
    const stop = subscribeTrace(store);
    serve("demo", demoTrace("After"));
    act(() => resyncHandlers.forEach((handler) => handler()));
    await waitFor(() => expect(titleOf(store)).toBe("After"));
    stop();
  });

  it("stops following the socket once unsubscribed", () => {
    const store = connected();
    subscribeTrace(store)();
    expect(changedHandlers.size).toBe(0);
    expect(resyncHandlers.size).toBe(0);
  });
});

describe("useTrace", () => {
  function Probe({ feature }: { feature: string }) {
    const { trace } = useTrace(feature);
    return createElement("p", { "data-testid": "title" }, trace?.documents[0]?.title ?? "none");
  }

  it("fetches on first use and follows trace_changed while mounted", async () => {
    serve("demo", demoTrace("First"));
    const store = connected();
    const view = render(
      createElement(Provider, { store }, createElement(Probe, { feature: "demo" })),
    );
    await waitFor(() => expect(view.getByTestId("title").textContent).toBe("First"));

    serve("demo", demoTrace("Second"));
    deliver("demo");
    await waitFor(() => expect(view.getByTestId("title").textContent).toBe("Second"));
    view.unmount();
    expect(changedHandlers.size).toBe(0);
  });

  it("fetches nothing for an empty feature", () => {
    const store = connected();
    render(createElement(Provider, { store }, createElement(Probe, { feature: "" })));
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
