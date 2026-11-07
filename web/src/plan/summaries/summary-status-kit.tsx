import { act, cleanup, render } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { initClient } from "@/plan/api/client";
import helloFixture from "@/plan/api/fixtures/hello.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import questionFixture from "@/plan/api/fixtures/summary-question.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { summaryPanelSchema, summaryQuestionSchema } from "@/plan/api/schema-summaries";
import type { connectPlanSocket } from "@/plan/api/socket";
import { authoringAtom, planSocketAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import { panelLoadAtom, type PanelLoad } from "./state";
import { SummaryStatus, SummaryStatusRow, useSummaryExpansion } from "./summary-status";

export type Message = Record<string, unknown>;
type Handler = (message: Message) => void;
export const PANEL_PATH = "/api/summaries/panel";
export const idle = summaryPanelSchema.parse({
  ...panelFixture,
  running: null,
  queue: [],
  failed: [],
  question: null,
  terminal_claims: [],
});
export const question = summaryQuestionSchema.parse(questionFixture);
export let handlers: Map<string, Set<Handler>>;
export let resyncs: Set<() => void>;
let replies: Array<{ status: number; body: unknown }>;
export let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>;

function fakeSocket(): ReturnType<typeof connectPlanSocket> {
  const subscribe = (type: string, handler: Handler) => {
    const listeners = handlers.get(type) ?? new Set<Handler>();
    handlers.set(type, listeners.add(handler));
    return () => void listeners.delete(handler);
  };
  const onResync = (handler: () => void) => {
    resyncs.add(handler);
    return () => void resyncs.delete(handler);
  };
  return { subscribe, onResync, send: () => true, dispose: () => {} } as unknown as ReturnType<
    typeof connectPlanSocket
  >;
}

function seededStore(canAnswer: boolean) {
  const store = createStore();
  initClient(store);
  store.set(youAtom, helloFixture.you);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...helloFixture.server, read_only: [] }));
  store.set(authoringAtom, {
    holder: {
      ...(canAnswer ? helloFixture.you : { public_id: "other-browser", name: "Dana" }),
      away_since: null,
      frees_at: null,
    },
  });
  store.set(planSocketAtom, fakeSocket());
  return store;
}

/** The activity popover owns the expansion; this stands in for it. */
function Row({ load }: { load: PanelLoad }) {
  return <SummaryStatusRow load={load} reload={() => {}} expansion={useSummaryExpansion()} />;
}

export function showRow(load: PanelLoad) {
  const store = seededStore(true);
  store.set(panelLoadAtom, load);
  render(
    <Provider store={store}>
      <TooltipProvider>
        <Row load={load} />
      </TooltipProvider>
    </Provider>,
  );
  return store;
}

export function show(compact = false, canAnswer = true) {
  const store = seededStore(canAnswer);
  render(
    <Provider store={store}>
      <TooltipProvider>
        <textarea aria-label="Message" />
        <SummaryStatus compact={compact} />
      </TooltipProvider>
    </Provider>,
  );
  return store;
}

export function deliver(type: string, body: Message) {
  act(() => handlers.get(type)?.forEach((handler) => handler({ type, ...body })));
}

export function visibleText(element: HTMLElement): string | null {
  const clone = element.cloneNode(true) as HTMLElement;
  clone.querySelectorAll(".sr-only").forEach((hidden) => hidden.remove());
  return clone.textContent;
}

export function answerPosts() {
  return fetchMock.mock.calls.filter(([url]) => String(url).includes("/api/summaries/answer"));
}

export function setReplies(next: Array<{ status: number; body: unknown }>) {
  replies = next;
}

/** Registers the fetch and socket doubles every summary status test relies on. */
export function installHarness() {
  beforeEach(() => {
    handlers = new Map();
    resyncs = new Set();
    replies = [{ status: 200, body: idle }];
    fetchMock = vi.fn<typeof fetch>(async () => {
      const reply = replies.length > 1 ? replies.shift() : replies[0];
      return new Response(JSON.stringify(reply?.body), {
        status: reply?.status ?? 500,
        headers: { "content-type": "application/json" },
      });
    });
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });
}
