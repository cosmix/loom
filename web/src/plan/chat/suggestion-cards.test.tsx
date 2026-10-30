import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { expect, it, vi } from "vitest";

import type { DocView } from "@/plan/api/schema-files";
import { pendingAttachmentsAtom, type AttachmentDraft } from "@/plan/attachments";
import { useChangedSince } from "@/plan/review/changed-since";
import { checkpointAtom } from "@/plan/review/checkpoint";
import { hashesAtom } from "@/plan/review/live";
import { authoringAtom, serverInfoAtom } from "@/plan/state/atoms";
import { draftAtom } from "./drafts";
import { storageKey } from "./storage";
import { SuggestionCards } from "./suggestion-cards";
import {
  Feed,
  installSuggestionLifecycle,
  mount,
  setup,
  socketFor,
  suggestion,
} from "./suggestion-cards-test-kit";

const document: DocView = {
  feature: null,
  misplaced: false,
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
  last_change: null,
  too_large: false,
  truncated: false,
  size: 48,
};

function fetchDocument(doc = document) {
  return vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(JSON.stringify(doc)));
}

async function attach() {
  fireEvent.click(screen.getByRole("button", { name: "Attach" }));
  await waitFor(() => expect(screen.queryByText("Suggestions (1)")).toBeNull());
}

installSuggestionLifecycle();

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
  // The live path keeps this list: it refetches `/api/hashes` on every `doc_changed`.
  store.set(hashesAtom, { head: "c1", docs: { [suggestion.passage.path]: "fresh-hash" } });
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

function ChangedSinceProbe() {
  useChangedSince();
  return null;
}

it("attach does not invent review hashes or a first checkpoint", async () => {
  fetchDocument();
  const { store, receive } = setup();
  mount(store);
  render(
    <Provider store={store}>
      <ChangedSinceProbe />
    </Provider>,
  );
  receive(suggestion);
  expect(store.get(hashesAtom)).toBeNull();

  await attach();

  expect(store.get(hashesAtom)).toBeNull();
  expect(store.get(checkpointAtom)).toBeNull();
  const projectKey = store.get(serverInfoAtom)!.project_key;
  expect(localStorage.getItem(storageKey(projectKey, "checkpoint"))).toBeNull();
});

it("attaching a summary passage names the summary by id", async () => {
  fetchDocument({
    ...document,
    path: "doc/loom/summaries/product.md",
    text: "Review loop passage.",
    sections: [],
  });
  const { store, receive } = setup();
  mount(store);
  receive({
    ...suggestion,
    passage: {
      path: "doc/loom/summaries/product.md",
      anchor: "",
      text: "Review loop passage.",
      lines: null,
      stage: null,
      summary_sentence: "Review loop",
    },
  });

  await attach();

  expect(store.get(pendingAttachmentsAtom)).toEqual([
    {
      kind: "summary",
      summary: "product",
      sentence: "Review loop",
      text: "Review loop passage.",
      restsOn: [],
      hash: "current-hash",
    },
  ]);
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
