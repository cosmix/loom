import { act, cleanup, render } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import questionFixture from "@/plan/api/fixtures/summary-question.json";
import { summaryPanelSchema, summaryQuestionSchema } from "@/plan/api/schema-summaries";
import type { connectPlanSocket } from "@/plan/api/socket";
import { planSocketAtom } from "@/plan/state/atoms";

import {
  summaryPanelAtom,
  summaryQuestionAtom,
  useSummaryActions,
  useSummaryFeed,
  type SummaryActions,
} from "./state";

type Message = Record<string, unknown>;
type Handler = (message: Message) => void;
const noQuestion = summaryPanelSchema.parse({ ...panelFixture, question: null });
const question = summaryQuestionSchema.parse(questionFixture);
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
  const pending = actions.rebuild(["product"]);
  deliver("summary_question", question);
  await resolveHeld(noQuestion);
  expect(await pending).toBeNull();
  expect(store.get(summaryQuestionAtom)?.id).toBe(question.id);
});

it("applies an action POST when no push arrived meanwhile", async () => {
  const store = show();
  await resolveHeld(noQuestion);
  const pending = actions.release(["product"]);
  await resolveHeld({ ...noQuestion, question });
  expect(await pending).toBeNull();
  expect(store.get(summaryQuestionAtom)?.id).toBe(question.id);
});
