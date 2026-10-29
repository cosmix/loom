import { act, cleanup, render } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import helloFixture from "@/plan/api/fixtures/hello.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import questionFixture from "@/plan/api/fixtures/summary-question.json";
import { planMessageSchema } from "@/plan/api/schema";
import { summaryPanelSchema, summaryQuestionSchema } from "@/plan/api/schema-summaries";
import type { connectPlanSocket } from "@/plan/api/socket";
import { autoRebuildAtom } from "@/plan/settings/preferences";
import {
  authoringAtom,
  connectionEpochAtom,
  planSocketAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";
import type { Store } from "@/state/store";

import {
  summaryPanelAtom,
  summaryQuestionAtom,
  useAutoRebuildSync,
  useSummaryActions,
  useSummaryFeed,
  type SummaryActions,
} from "./state";

type Message = Record<string, unknown>;
type Handler = (message: Message) => void;
const noQuestion = summaryPanelSchema.parse({ ...panelFixture, question: null });
const question = summaryQuestionSchema.parse(questionFixture);
const hello = planMessageSchema.parse(helloFixture);
let handlers: Map<string, Set<Handler>>;
let held: Array<(response: Response) => void>;
let actions: SummaryActions;

function fakeSocket(): ReturnType<typeof connectPlanSocket> {
  const subscribe = (type: string, handler: Handler) => {
    const listeners = handlers.get(type) ?? new Set<Handler>();
    handlers.set(type, listeners.add(handler));
    return () => void listeners.delete(handler);
  };
  return {
    subscribe,
    onResync: () => () => {},
    send: () => true,
    dispose: () => {},
  } as unknown as ReturnType<typeof connectPlanSocket>;
}

function Harness() {
  useSummaryFeed();
  actions = useSummaryActions();
  return null;
}

function AutoRebuildHarness() {
  useAutoRebuildSync();
  return null;
}

function showAutoSync() {
  const store = createStore();
  store.set(planSocketAtom, fakeSocket());
  render(
    <Provider store={store}>
      <AutoRebuildHarness />
    </Provider>,
  );
  return store;
}

function deliverHello(store: Store, who: "holder" | "other" | "read_only" = "holder") {
  if (hello.type !== "hello") throw new Error("Expected a hello fixture");
  act(() => {
    store.set(youAtom, hello.you);
    store.set(serverInfoAtom, {
      ...hello.server,
      read_only: who === "read_only" ? hello.server.read_only : [],
    });
    store.set(authoringAtom, {
      holder: {
        ...hello.authoring.holder!,
        public_id: who === "other" ? "other-browser" : hello.you.public_id,
      },
    });
  });
}

function show() {
  const store = createStore();
  store.set(planSocketAtom, fakeSocket());
  render(
    <Provider store={store}>
      <Harness />
    </Provider>,
  );
  return store;
}

function deliver(type: string, body: Message) {
  act(() => handlers.get(type)?.forEach((handler) => handler({ type, ...body })));
}

async function resolveHeld(body: unknown) {
  await vi.waitFor(() => expect(held.length).toBeGreaterThan(0));
  const respond = held.shift();
  if (!respond) throw new Error("No request is held open");
  await act(async () =>
    respond(
      new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    ),
  );
}

beforeEach(() => {
  localStorage.clear();
  handlers = new Map();
  held = [];
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(
      () =>
        new Promise<Response>((resolve) => {
          held.push(resolve);
        }),
    ),
  );
});

afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.unstubAllGlobals();
});

it("applies the seed GET when no push arrived meanwhile", async () => {
  const store = show();
  await resolveHeld(panelFixture);
  expect(store.get(summaryPanelAtom)?.running?.session).toBe(panelFixture.running.session);
});

it("keeps a question pushed while the seed GET was in flight", async () => {
  const store = show();
  deliver("summary_panel", noQuestion);
  deliver("summary_question", question);
  expect(store.get(summaryQuestionAtom)?.id).toBe(question.id);
  await resolveHeld(noQuestion);
  expect(store.get(summaryQuestionAtom)?.id).toBe(question.id);
});

it("keeps a question pushed while an action POST was in flight, and reports success", async () => {
  const store = show();
  await resolveHeld(noQuestion);
  const pending = actions.rebuild(["overview/demo/product"]);
  deliver("summary_question", question);
  await resolveHeld(noQuestion);
  expect(await pending).toBeNull();
  expect(store.get(summaryQuestionAtom)?.id).toBe(question.id);
});

it("applies an action POST when no push arrived meanwhile", async () => {
  const store = show();
  await resolveHeld(noQuestion);
  const pending = actions.release(["overview/demo/product"]);
  await resolveHeld({ ...noQuestion, question });
  expect(await pending).toBeNull();
  expect(store.get(summaryQuestionAtom)?.id).toBe(question.id);
});

it("reports rebuild action errors without replacing the panel", async () => {
  const store = show();
  await resolveHeld(noQuestion);
  vi.mocked(fetch).mockRejectedValueOnce(new TypeError("Failed to fetch"));

  expect(await actions.rebuild(["overview/demo/product"])).toBe("Failed to fetch");
  expect(store.get(summaryPanelAtom)).toEqual(noQuestion);
});

it("syncs the holder's auto-rebuild preference after hello and each toggle", async () => {
  const store = showAutoSync();
  expect(fetch).not.toHaveBeenCalled();

  deliverHello(store);
  await resolveHeld(noQuestion);
  act(() => store.set(autoRebuildAtom, false));
  await resolveHeld(noQuestion);
  act(() => store.set(autoRebuildAtom, true));
  await resolveHeld(noQuestion);

  expect(vi.mocked(fetch).mock.calls).toEqual([
    ["/api/summaries/auto", expect.objectContaining({ method: "POST", body: '{"enabled":true}' })],
    ["/api/summaries/auto", expect.objectContaining({ method: "POST", body: '{"enabled":false}' })],
    ["/api/summaries/auto", expect.objectContaining({ method: "POST", body: '{"enabled":true}' })],
  ]);
});

it.each(["other", "read_only"] as const)("does not sync auto-rebuild for %s browsers", (who) => {
  const store = showAutoSync();
  deliverHello(store, who);

  act(() => store.set(autoRebuildAtom, false));
  act(() => store.set(autoRebuildAtom, true));

  expect(fetch).not.toHaveBeenCalled();
});

it("resends the current auto-rebuild preference after a reconnect hello", async () => {
  const store = showAutoSync();
  deliverHello(store);
  act(() => store.set(autoRebuildAtom, false));
  await resolveHeld(noQuestion);
  await resolveHeld(noQuestion);

  act(() => store.set(connectionEpochAtom, (epoch) => epoch + 1));
  await resolveHeld(noQuestion);

  expect(fetch).toHaveBeenCalledTimes(3);
  expect(fetch).toHaveBeenLastCalledWith(
    "/api/summaries/auto",
    expect.objectContaining({ method: "POST", body: '{"enabled":false}' }),
  );
});
