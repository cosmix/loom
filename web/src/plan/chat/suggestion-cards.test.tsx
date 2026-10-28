import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import type { DocView } from "@/plan/api/schema-files";
import type { connectPlanSocket } from "@/plan/api/socket";
import { pendingAttachmentsAtom, type AttachmentDraft } from "@/plan/attachments";
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
const suggestion: Suggestion = {
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
const document: DocView = {
  path: suggestion.passage.path,
  title: "Ports",
  text: "## Port\nOld port text.\n\n## Other\nOther text.",
  hash: "current-hash",
  kind: "section",
  editable: true,
  lock: null,
  sections: [
    { level: 2, title: "Port", anchor: "port", start_line: 1, end_line: 3 },
    { level: 2, title: "Other", anchor: "other", start_line: 4, end_line: 5 },
  ],
  plan: null,
  too_large: false,
  truncated: false,
  size: 48,
};

function socketFor(store: Store) {
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

function setup(holder = true) {
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
function Feed() {
  useSuggestionFeed();
  return null;
}

function mount(store: Store) {
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

function fetchDocument(doc = document) {
  return vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(JSON.stringify(doc)));
}

async function attach() {
  fireEvent.click(screen.getByRole("button", { name: "Attach" }));
  await waitFor(() => expect(screen.queryByText("Suggestions (1)")).toBeNull());
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

it("attach fills chip and composer", async () => {
  fetchDocument();
  const { store, receive } = setup();
  store.set(draftAtom, { text: "Existing draft", attachments: [] });
  mount(store);
  receive(suggestion);

  await attach();

  expect(screen.getByRole("button", { name: "section_02.md › port" })).toBeTruthy();
  expect(store.get(draftAtom)).toEqual({
    text: "Existing draft\n\nSam asks: Clarify this port",
    attachments: [
      {
        kind: "source",
        path: "doc/product/section_02.md",
        anchor: "port",
        text: "Old port text.",
        lines: null,
        stageId: null,
        hash: "current-hash",
      },
    ],
  });
  expect(store.get(pendingAttachmentsAtom)).toEqual(store.get(draftAtom).attachments);
  expect(screen.queryByText("△ changed")).toBeNull();
});

it("attach re-reads the document and marks changed text", async () => {
  const fetch = fetchDocument({
    ...document,
    text: "## Port\nNew port text.\n\n## Other\nOther text.",
    hash: "fresh-hash",
  });
  const { store, receive } = setup();
  mount(store);
  receive(suggestion);
  expect(fetch).not.toHaveBeenCalled();

  await attach();

  expect(fetch).toHaveBeenCalledWith(
    "/api/doc?path=product%2Fsection_02.md",
    expect.objectContaining({ method: "GET" }),
  );
  expect(store.get(pendingAttachmentsAtom)[0]).toMatchObject({
    text: "## Port\nNew port text.\n",
    hash: "",
    lines: null,
  });
  expect(screen.getByText("△ changed")).toBeTruthy();
});

it("attach sends no chat command", async () => {
  fetchDocument();
  const { store, receive, send } = setup();
  mount(store);
  receive(suggestion);

  await attach();

  expect(send.mock.calls).toEqual([
    [{ type: "review", kind: "suggestion_answer", id: "sg-1", action: "attach" }],
  ]);
});

it("dismiss answers", async () => {
  const fetch = vi.spyOn(globalThis, "fetch");
  const { store, receive, send } = setup();
  mount(store);
  receive(suggestion);

  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));

  await waitFor(() => expect(screen.queryByText("Suggestions (1)")).toBeNull());
  expect(send).toHaveBeenCalledWith({
    type: "review",
    kind: "suggestion_answer",
    id: "sg-1",
    action: "dismiss",
  });
  expect(fetch).not.toHaveBeenCalled();
  expect(store.get(draftAtom)).toEqual({ text: "", attachments: [] });
});

it("note renders as text", () => {
  const { store, receive } = setup();
  const { container } = mount(store);
  receive({ ...suggestion, from: "<b>Sam</b>\u202e", note: "<img src=x onerror=alert(1)>\u200b" });

  expect(screen.getByText("<img src=x onerror=alert(1)>⟨U+200B⟩")).toBeTruthy();
  expect(screen.getByText("<b>Sam</b>⟨U+202E⟩")).toBeTruthy();
  expect(container.querySelector("img, b")).toBeNull();
});

it("only the holder sees suggestion cards, including when authoring is free", () => {
  const { store, receive } = setup(false);
  mount(store);
  receive(suggestion);
  expect(screen.queryByRole("button", { name: "Attach" })).toBeNull();

  act(() => store.set(authoringAtom, { holder: null }));

  expect(screen.queryByText("Suggestions (1)")).toBeNull();
});

it("answered statuses remove cards while sent status keeps them", () => {
  const { store, receive } = setup();
  mount(store);
  receive(suggestion);
  receive({ kind: "suggestion_status", id: "sg-1", state: "sent", holder: "Me", away: false });
  expect(screen.getByText("Suggestions (1)")).toBeTruthy();

  receive({ kind: "suggestion_status", id: "sg-1", state: "dropped", reason: "Author changed" });

  expect(screen.queryByText("Suggestions (1)")).toBeNull();
});

it("socket replacement and unmount clean up the review subscription", () => {
  const { store, receive, listeners } = setup();
  const view = mount(store);
  let replacement!: ReturnType<typeof socketFor>;
  act(() => {
    replacement = socketFor(store);
  });
  receive(suggestion);
  expect(screen.queryByText("Suggestions (1)")).toBeNull();
  replacement.receive(suggestion);
  expect(screen.getByText("Suggestions (1)")).toBeTruthy();
  expect(listeners.get("review")?.size).toBe(0);

  view.unmount();

  expect(replacement.listeners.get("review")?.size).toBe(0);
});

it("a suggestion received while the cards are unmounted appears when they mount", () => {
  const { store, receive } = setup();
  render(
    <Provider store={store}>
      <Feed />
    </Provider>,
  );
  receive(suggestion);
  render(
    <Provider store={store}>
      <MemoryRouter>
        <SuggestionCards />
      </MemoryRouter>
    </Provider>,
  );
  expect(screen.getByText("Suggestions (1)")).toBeTruthy();
});

it("failed document reads keep the suggestion and draft", async () => {
  vi.spyOn(globalThis, "fetch").mockResolvedValue(
    new Response(JSON.stringify({ error: "missing", message: "Document is gone" }), {
      status: 404,
    }),
  );
  const { store, receive, send } = setup();
  mount(store);
  receive(suggestion);

  fireEvent.click(screen.getByRole("button", { name: "Attach" }));

  expect(await screen.findByText("Document is gone")).toBeTruthy();
  expect(screen.getByText("Suggestions (1)")).toBeTruthy();
  expect(store.get(draftAtom)).toEqual({ text: "", attachments: [] });
  expect(send).not.toHaveBeenCalled();
});

it("losing authoring during a read prevents attaching or answering", async () => {
  let finish!: (response: Response) => void;
  vi.spyOn(globalThis, "fetch").mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const { store, receive, send } = setup();
  mount(store);
  receive(suggestion);
  fireEvent.click(screen.getByRole("button", { name: "Attach" }));
  act(() => store.set(authoringAtom, { holder: null }));

  await act(async () => {
    finish(new Response(JSON.stringify(document)));
  });

  expect(store.get(draftAtom)).toEqual({ text: "", attachments: [] });
  expect(send).not.toHaveBeenCalled();
});

it("20 suggestions use a capped disclosure with the newest card first", () => {
  const { store, receive } = setup();
  mount(store);
  for (let index = 1; index <= 20; index++)
    receive({ ...suggestion, id: `sg-${index}`, note: `Note ${index}` });

  const summary = screen.getByText("Suggestions (20)");
  const list = screen.getByRole("region", { name: "Pending suggestions" });
  expect(list.className).toContain("max-h-[33vh]");
  expect(list.className).toContain("overflow-y-auto");
  expect(list.querySelector("article")?.textContent).toContain("Note 20");
  expect(summary.closest("details")?.open).toBe(true);
});

it("a full attachment draft is kept without answering the suggestion", async () => {
  fetchDocument();
  const { store, receive, send } = setup();
  const attachments: AttachmentDraft[] = Array.from({ length: 10 }, (_, index) => ({
    kind: "source",
    path: `doc/${index}.md`,
    anchor: null,
    text: "Selected",
    lines: null,
    stageId: null,
    hash: null,
  }));
  store.set(draftAtom, { text: "Keep this", attachments });
  mount(store);
  receive(suggestion);

  fireEvent.click(screen.getByRole("button", { name: "Attach" }));

  expect(await screen.findByText("At most 10 attachments per message")).toBeTruthy();
  expect(store.get(draftAtom)).toEqual({ text: "Keep this", attachments });
  expect(send).not.toHaveBeenCalled();
});
