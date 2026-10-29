import { act, cleanup, render } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, vi } from "vitest";

import type { connectPlanSocket } from "@/plan/api/socket";
import type { ReviewMessage } from "@/plan/review/api";
import {
  authoringAtom,
  connectionAtom,
  planSocketAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";
import { PendingChips } from "./chips";
import { draftAtom } from "./drafts";
import { SuggestionCards, useSuggestionFeed } from "./suggestion-cards";

type Suggestion = Extract<ReviewMessage, { kind: "suggestion" }>;
type Store = ReturnType<typeof createStore>;
let project = 0;
export const suggestion: Suggestion = {
  kind: "suggestion",
  id: "sg-1",
  from: "Sam",
  passage: {
    path: "doc/product/section_02.md",
    anchor: "port",
    text: "Old port text.",
    lines: null,
    stage: null,
    summary_sentence: null,
  },
  note: "Clarify this port",
};

export function socketFor(store: Store) {
  const listeners = new Map<string, Set<(message: unknown) => void>>();
  const receive = (message: ReviewMessage) => {
    act(() => {
      [...(listeners.get("review") ?? [])].forEach((handler) =>
        handler({ type: "review", ...message }),
      );
    });
  };
  const send = vi.fn((message: unknown) => {
    const request = message as { type: string; kind?: string; id?: string; action?: string };
    if (request.type === "review" && request.kind === "suggestion_answer") {
      queueMicrotask(() =>
        receive({
          kind: "suggestion_status",
          id: request.id!,
          state: request.action === "attach" ? "attached" : "dismissed",
        }),
      );
    }
    return true;
  });
  const socket = {
    send,
    subscribe: (type: string, handler: (message: unknown) => void) => {
      const handlers = listeners.get(type) ?? new Set();
      listeners.set(type, handlers);
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
      };
    },
    onResync: () => () => {},
    dispose: () => {},
  };
  store.set(planSocketAtom, socket as unknown as ReturnType<typeof connectPlanSocket>);
  return { send, receive, listeners };
}

export function setup(holder = true) {
  const store = createStore();
  store.set(serverInfoAtom, {
    project: "p",
    project_key: `suggestions-${++project}`,
    port: 1,
    remote: false,
    stopping: false,
    read_only: [],
    you: { public_id: "me", name: "Me" },
  });
  store.set(youAtom, { public_id: "me", name: "Me" });
  store.set(authoringAtom, {
    holder: {
      public_id: holder ? "me" : "other",
      name: "Author",
      away_since: null,
      frees_at: null,
    },
  });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: null });
  store.set(draftAtom, { text: "", attachments: [] });
  return { store, ...socketFor(store) };
}

// The panel mounts the feed beside the cards; the cards may be unmounted while it listens.
export function Feed() {
  useSuggestionFeed();
  return null;
}

export function mount(store: Store) {
  return render(
    <Provider store={store}>
      <MemoryRouter>
        <Feed />
        <SuggestionCards />
        <PendingChips />
      </MemoryRouter>
    </Provider>,
  );
}

/** Registers the per-test reset; call once at the top of each suggestion-cards test file. */
export function installSuggestionLifecycle() {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    localStorage.clear();
  });
}
