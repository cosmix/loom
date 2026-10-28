import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { connectPlanSocket } from "@/plan/api/socket";
import { AddToChat, type AttachmentDraft } from "@/plan/attachments";
import { authoringAtom, planSocketAtom, youAtom } from "@/plan/state/atoms";

import { registerSendToAuthor } from "./send-to-author";

const me = { public_id: "viewer", name: "Viewer" };
const ada = { public_id: "author", name: "Ada", away_since: null, frees_at: null };
const selection: AttachmentDraft = {
  kind: "source",
  path: "doc/product/section_10.md",
  anchor: "review",
  text: "Selected passage",
  lines: { start: 3, end: 5 },
  stageId: null,
  hash: "abc",
};
const rect = { top: 10, left: 10, right: 50, bottom: 30, width: 40, height: 20 } as DOMRect;
let unregister: (() => void) | undefined;

beforeEach(() => {
  unregister = registerSendToAuthor();
  vi.spyOn(crypto, "randomUUID").mockReturnValue("00000000-0000-4000-8000-000000000001");
});

afterEach(() => {
  cleanup();
  act(() => unregister?.());
  vi.restoreAllMocks();
});

function harness(draft = selection) {
  const store = createStore();
  store.set(youAtom, me);
  store.set(authoringAtom, { holder: ada });
  const listeners = new Map<string, Set<(message: unknown) => void>>();
  const send = vi.fn((_message: unknown) => true);
  const socket = {
    send,
    subscribe(type: string, handler: (message: unknown) => void) {
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
  const view = render(
    <Provider store={store}>
      <AddToChat draft={draft} rect={rect} onAdded={() => {}} />
    </Provider>,
  );
  const receive = (message: object) =>
    act(() => {
      [...(listeners.get("review") ?? [])].forEach((handler) =>
        handler({ type: "review", ...message }),
      );
    });
  return { store, send, receive, view };
}

async function openForm() {
  fireEvent.click(screen.getByRole("button", { name: "Send to author" }));
  return screen.findByRole("dialog", { name: "Send to author" });
}

async function sendPassage() {
  await openForm();
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
}

function sent(receive: (message: object) => void, away = false) {
  receive({ kind: "suggestion_status", id: "mine", state: "sent", holder: "Ada", away });
}

it("shown only to viewers while held", () => {
  const { store } = harness();
  expect(screen.getByRole("button", { name: "Send to author" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Add to chat" })).toBeNull();
  act(() => store.set(youAtom, { public_id: ada.public_id, name: ada.name }));
  expect(screen.queryByRole("button", { name: "Send to author" })).toBeNull();
  expect(screen.getByRole("button", { name: "Add to chat" })).toBeTruthy();
  act(() => store.set(youAtom, me));
  expect(screen.getByRole("button", { name: "Send to author" })).toBeTruthy();
});

it("form limits", async () => {
  const { send } = harness();
  await openForm();
  expect(screen.getByText(/doc\/product\/section_10.md#review/)).toBeTruthy();
  const note = screen.getByRole("textbox", { name: "Note (optional)" });
  fireEvent.change(note, { target: { value: "😀".repeat(1_001) } });
  const button = screen.getByRole("button", { name: "Send" }) as HTMLButtonElement;
  expect(button.disabled).toBe(true);
  fireEvent.submit(button.closest("form")!);
  expect(send).not.toHaveBeenCalled();
  expect(screen.getByText(/1,001 \/ 1,000/)).toBeTruthy();
  fireEvent.change(note, { target: { value: "😀".repeat(1_000) } });
  expect(button.disabled).toBe(false);
  fireEvent.click(button);
  expect(send).toHaveBeenCalledWith({
    type: "review",
    kind: "suggest",
    note: "😀".repeat(1_000),
    passage: {
      path: selection.path,
      anchor: "review",
      text: "Selected passage",
      lines: [3, 5],
      stage: null,
      summary_sentence: null,
    },
  });
});

it.each(["attached", "dismissed"] as const)("statuses", async (state) => {
  const { receive } = harness();
  await sendPassage();
  sent(receive);
  expect(await screen.findByText("Sent to Ada")).toBeTruthy();
  receive({ kind: "suggestion_status", id: "someone-else", state });
  expect(screen.getByText("Sent to Ada")).toBeTruthy();
  receive({ kind: "suggestion_status", id: "mine", state });
  expect(await screen.findByText(state === "attached" ? "Attached" : "Dismissed")).toBeTruthy();
});

it("away status", async () => {
  const { store, receive } = harness();
  act(() => store.set(authoringAtom, { holder: { ...ada, away_since: "2026-10-28T12:00:00Z" } }));
  await sendPassage();
  expect(screen.getByText("Ada is away")).toBeTruthy();
  sent(receive, true);
  expect(await screen.findByText("Sent to Ada")).toBeTruthy();
  expect(screen.getByText("Ada is away")).toBeTruthy();
});

it("hidden without holder", () => {
  const { store } = harness();
  act(() => store.set(authoringAtom, { holder: null }));
  expect(screen.queryByRole("button", { name: "Send to author" })).toBeNull();
});

it("dropped status shown to the sender", async () => {
  const { receive, store, view } = harness();
  await sendPassage();
  sent(receive);
  await screen.findByText("Sent to Ada");
  // The selection and its button disappear while the independent sender dialog stays open.
  view.unmount();
  act(() => store.set(authoringAtom, { holder: null }));
  receive({
    kind: "suggestion_status",
    id: "mine",
    state: "dropped",
    reason: "authoring changed hands",
  });
  expect(await screen.findByText("Dropped: authoring changed hands")).toBeTruthy();
});

it("refusals retain the note for retry", async () => {
  const { receive, send } = harness();
  await openForm();
  const note = screen.getByRole("textbox", { name: "Note (optional)" }) as HTMLTextAreaElement;
  fireEvent.change(note, { target: { value: "Please clarify" } });
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  receive({ kind: "refused", reason: "Try again" });
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Try again");
  expect(note.value).toBe("Please clarify");
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  expect(send).toHaveBeenCalledTimes(2);
});

it("captures summary provenance and allows an empty note", async () => {
  const { send } = harness({
    kind: "summary",
    summary: "product",
    sentence: "Review loop",
    text: "Summary passage",
    restsOn: [],
    hash: null,
  });
  await sendPassage();
  expect(send).toHaveBeenCalledWith({
    type: "review",
    kind: "suggest",
    note: "",
    passage: {
      path: "doc/loom/summaries/product.md",
      anchor: "Review loop",
      text: "Summary passage",
      lines: null,
      stage: null,
      summary_sentence: "Review loop",
    },
  });
});

it("authoring changes disable an unsent form", async () => {
  const { store, send } = harness();
  await openForm();
  act(() => store.set(authoringAtom, { holder: null }));
  const button = screen.getByRole("button", { name: "Send" }) as HTMLButtonElement;
  expect(button.disabled).toBe(true);
  fireEvent.submit(button.closest("form")!);
  expect(send).not.toHaveBeenCalled();
});

it("unregister removes the sender dialog and its status subscription", async () => {
  harness();
  await openForm();
  act(() => unregister?.());
  unregister = undefined;
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});
