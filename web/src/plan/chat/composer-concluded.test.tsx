import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { expect, it } from "vitest";

import { initClient } from "@/plan/api/client";
import type { SessionState } from "@/plan/api/schema-chat";
import { box, initial, installComposerLifecycle, mount, send, setup } from "./composer-harness";
import { installDocServer, pathOf, pathsAt, sentAt, view } from "./composer-context-kit";
import { lastAttachedAtom } from "./context-attach";
import { draftAtom } from "./drafts";
import { queueReturnedAtom, snapshotAtom, unconfirmedSendsAtom } from "./model";

installComposerLifecycle();
installDocServer();

const FIRST_PROMPT = "What does this section decide?";
const NEW_CHAT_NOTE = "Sending starts a new chat";

const prompts = () => screen.queryByRole("list", { name: "Suggested prompts" });
const newChat = () => screen.queryByRole("button", { name: /new chat/i });
const newSession = (store: ReturnType<typeof setup>["store"], id: string, state: SessionState) =>
  act(() => store.set(snapshotAtom, (s) => ({ ...s, session: { ...s.session!, id, state } })));

/** A concluded chat whose panel shows section 05, with a prompt clicked. */
function chosen() {
  const h = setup("ended");
  initClient(h.store);
  view(h, "05");
  mount(h.store);
  fireEvent.click(screen.getByRole("button", { name: FIRST_PROMPT }));
  return h;
}

it("a prompt clicked on a concluded chat shows the box, filled and focused, with New chat beside it", async () => {
  chosen();

  expect(box().value).toBe(FIRST_PROMPT);
  await waitFor(() => expect(document.activeElement).toBe(box()));
  expect(newChat()).not.toBeNull();
  expect(screen.getByText(NEW_CHAT_NOTE).className).toContain("plan-fine");
});

it("sending from it starts a new chat that carries the viewed document", async () => {
  const h = setup("ended");
  initClient(h.store);
  view(h, "05");
  // The dead chat was given this document; the new one has been given nothing.
  act(() => h.store.set(lastAttachedAtom, { path: pathOf("05"), session: initial.session!.id }));
  mount(h.store);
  fireEvent.click(screen.getByRole("button", { name: FIRST_PROMPT }));
  expect(h.sent).toEqual([]);

  fireEvent.click(send());

  await waitFor(() => expect(h.sent).toHaveLength(1));
  expect(sentAt(h, 0).command).toBe("start");
  expect(pathsAt(h, 0)).toEqual([pathOf("05")]);
});

it("focus on New chat beside the box moves to the box when the next chat starts", async () => {
  const h = chosen();
  const button = newChat()!;
  button.focus();
  expect(document.activeElement).toBe(button);

  newSession(h.store, "00000000-0000-4000-8000-000000000099", "idle");

  expect(newChat()).toBeNull();
  await waitFor(() => expect(document.activeElement).toBe(box()));
});

it("emptying the box keeps it mounted and focused, and the prompts return", async () => {
  chosen();
  await waitFor(() => expect(document.activeElement).toBe(box()));
  expect(prompts()).toBeNull();

  fireEvent.change(box(), { target: { value: "" } });

  expect(box().value).toBe("");
  expect(document.activeElement).toBe(box());
  expect(prompts()).not.toBeNull();
});

it("the box stays mounted and focused from the start's result until the snapshot", async () => {
  const h = chosen();
  await waitFor(() => expect(document.activeElement).toBe(box()));

  fireEvent.keyDown(box(), { key: "Enter" });
  await waitFor(() => expect(h.sent).toHaveLength(1));
  await h.result();

  // The draft cleared on the result while the snapshot still reports the ended chat.
  await waitFor(() => expect(h.store.get(draftAtom).text).toBe(""));
  expect(box()).toBe(document.activeElement);

  newSession(h.store, "fresh", "idle");
  expect(box()).toBe(document.activeElement);
  expect(newChat()).toBeNull();
  expect(screen.queryByText(NEW_CHAT_NOTE)).toBeNull();
});

it("a stale draft on a concluded chat leaves New chat in place, with no box to press Enter in", () => {
  const h = setup("ended");
  view(h, "05");
  h.store.set(draftAtom, { text: "Words left from an earlier chat", attachments: [] });
  mount(h.store);

  expect(screen.queryByRole("textbox", { name: "Message" })).toBeNull();
  expect(newChat()).not.toBeNull();
  expect(screen.queryByText(NEW_CHAT_NOTE)).toBeNull();
});

it("a queue returned to a concluded chat fills the draft and leaves New chat in place", async () => {
  const h = setup("ended");
  mount(h.store);

  act(() => h.store.set(queueReturnedAtom, ["Returned words"]));

  await waitFor(() => expect(h.store.get(draftAtom).text).toBe("Returned words"));
  expect(screen.queryByRole("textbox", { name: "Message" })).toBeNull();
  expect(newChat()).not.toBeNull();
});

it("the choice belongs to the chat it was made on and does not reach the next chat's end", () => {
  const h = chosen();
  expect(box()).toBeTruthy();

  newSession(h.store, "another-chat", "idle");
  newSession(h.store, "another-chat", "ended");

  expect(screen.queryByRole("textbox", { name: "Message" })).toBeNull();
  expect(newChat()).not.toBeNull();
});

it("offers no Resend on a concluded chat's composer", () => {
  const h = chosen();
  act(() =>
    h.store.set(unconfirmedSendsAtom, [{ client_id: "lost", text: "Old text", attachments: [] }]),
  );

  expect(box()).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();
});
