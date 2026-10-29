import { act, cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import chatFixture from "@/plan/api/fixtures/chat-snapshot.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { chatSnapshotSchema, type ChatSnapshot } from "@/plan/api/schema-chat";
import type { connectPlanSocket } from "@/plan/api/socket";
import { planSocketAtom } from "@/plan/state/atoms";
import { useSummaryFeed } from "@/plan/summaries/state";

import { snapshotAtom } from "./model";
import { RebuiltSummaryExtras } from "./rebuilt-summary-extras";

type Handler = (message: Record<string, unknown>) => void;
let handlers: Map<string, Set<Handler>>;

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

function cost(session: string, rows: Array<{ id: string; tokens?: number }>) {
  const message = { type: "summary_turn_cost", session, turn: "t1", rows };
  act(() => handlers.get("summary_turn_cost")?.forEach((handler) => handler(message)));
}

function Feed() {
  useSummaryFeed();
  return null;
}

/** The fixture's previous and current chats both hold a turn `t1`: two objects, one id. */
function show(): ChatSnapshot {
  const snapshot = chatSnapshotSchema.parse(structuredClone(chatFixture));
  const previous = snapshot.previous;
  if (!previous || !snapshot.session)
    throw new Error("Fixture needs a previous and a current chat");
  expect(previous.turns[0].id).toBe(snapshot.turns[0].id);
  expect(previous.session.id).not.toBe(snapshot.session.id);
  const store = createStore();
  store.set(snapshotAtom, snapshot);
  store.set(planSocketAtom, fakeSocket());
  render(
    <Provider store={store}>
      <Feed />
      <div data-testid="previous">
        <RebuiltSummaryExtras id="product" turn={previous.turns[0]} />
      </div>
      <div data-testid="current">
        <RebuiltSummaryExtras id="product" turn={snapshot.turns[0]} />
      </div>
    </Provider>,
  );
  return snapshot;
}

beforeEach(() => {
  handlers = new Map();
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(
      async () =>
        new Response(JSON.stringify(panelFixture), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
    ),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("renders nothing until a cost message arrives", async () => {
  show();
  await act(async () => {});
  expect(screen.getByTestId("previous").textContent).toBe("");
  expect(screen.getByTestId("current").textContent).toBe("");
});

it("shows each chat's own figure for a turn id both chats hold", async () => {
  const snapshot = show();
  await act(async () => {});
  cost(snapshot.previous?.session.id ?? "", [{ id: "product", tokens: 1111 }]);
  cost(snapshot.session?.id ?? "", [{ id: "product", tokens: 2222 }]);
  expect(screen.getByTestId("previous").textContent).toBe("1,111 tokens");
  expect(screen.getByTestId("current").textContent).toBe("2,222 tokens");
});

it("shows tokens unknown for a row delivered without tokens, in its own chat only", async () => {
  const snapshot = show();
  await act(async () => {});
  cost(snapshot.session?.id ?? "", [{ id: "product" }]);
  expect(screen.getByTestId("current").textContent).toBe("tokens unknown");
  expect(screen.getByTestId("previous").textContent).toBe("");
});
