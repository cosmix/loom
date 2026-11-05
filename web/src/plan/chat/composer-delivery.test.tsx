import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { getDefaultStore } from "jotai";
import { expect, it } from "vitest";

import suggestion from "@/plan/api/fixtures/review-suggestion.json";
import { addAttachmentAtom } from "@/plan/attachments";
import { authoringAtom, connectionAtom, serverInfoAtom } from "@/plan/state/atoms";
import {
  box,
  initial,
  installComposerLifecycle,
  me,
  mount,
  other,
  permissions,
  queue,
  send,
  setup,
  ShortcutHost,
  source,
  type,
} from "./composer-harness";
import { endChat } from "./commands";
import { ComposerArea } from "./composer";
import { draftAtom } from "./drafts";
import { snapshotAtom } from "./model";
import { Transcript } from "./transcript";
import { chatSheetOpenAtom } from "./sheet-state";
import { storageFullAtom } from "./storage";

installComposerLifecycle();

it("draft kept when the send is rejected", async () => {
  const h = setup();
  mount(h.store);
  type();
  act(() => h.store.set(addAttachmentAtom, source()));
  fireEvent.click(send());
  await h.result("Refused");
  expect(h.store.get(draftAtom)).toMatchObject({ text: "Hello", attachments: [source()] });
});
it("acceptance preserves edits made while sending", async () => {
  const h = setup();
  mount(h.store);
  type();
  act(() => h.store.set(addAttachmentAtom, source()));
  fireEvent.click(send());
  type("New draft");
  act(() => h.store.set(addAttachmentAtom, source(1)));
  await h.result();
  expect(h.store.get(draftAtom)).toEqual({ text: "New draft", attachments: [source(1)] });
});
it("resend offered only for an unconfirmed send", async () => {
  const h = setup();
  mount(h.store);
  type();
  fireEvent.click(send());
  await h.result("Refused");
  expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();
  fireEvent.click(send());
  act(() =>
    h.store.set(connectionAtom, { phase: "reconnecting", everOpen: true, lastMessageAt: null }),
  );
  await screen.findByRole("button", { name: "Resend" });
  expect(box().value).toBe("Hello");
  h.receive("chat", {
    frame: "snapshot",
    ...h.store.get(snapshotAtom),
    turns: [
      {
        ...initial.turns[0],
        items: [
          {
            type: "user",
            id: "delivered",
            text: "Hello",
            attachments: [],
            delivery: "delivered",
            origin: null,
          },
        ],
      },
    ],
  });
  expect(screen.queryByRole("button", { name: "Resend" })).toBeNull();
});
it("losing authoring keeps the draft and shows the read-only line", () => {
  const h = setup();
  mount(h.store);
  type("Keep me");
  act(() => h.store.set(authoringAtom, { holder: other }));
  expect(screen.getByText("Your draft is kept")).toBeTruthy();
  expect(screen.queryByRole("textbox")).toBeNull();
  act(() => h.store.set(authoringAtom, { holder: { ...other, ...me } }));
  expect(box().value).toBe("Keep me");
});
it("storage full note", () => {
  const h = setup();
  mount(h.store);
  act(() => getDefaultStore().set(storageFullAtom, true));
  expect(screen.getByText("Unsaved text is not being kept: browser storage is full")).toBeTruthy();
});
it("second pending permission takes the card after the first is answered", () => {
  const h = setup("permission");
  permissions(h.store);
  mount(h.store, <Transcript />);
  expect(screen.getByRole("region", { name: "Permission: Bash 0" })).toBeTruthy();
  act(() =>
    h.store.set(snapshotAtom, (s) => ({
      ...s,
      turns: s.turns.map((t) => ({
        ...t,
        items: t.items.map((item) =>
          item.id === "p0" && item.type === "permission" ? { ...item, status: "allowed" } : item,
        ),
      })),
    })),
  );
  expect(screen.getByRole("region", { name: "Permission: Bash 1" })).toBeTruthy();
  expect(screen.queryByRole("region", { name: "Permission: Bash 0" })).toBeNull();
});
it("1 of 2 decisions", () => {
  const h = setup("permission");
  permissions(h.store);
  mount(h.store, <Transcript />);
  expect(screen.getByText("1 of 2 decisions")).toBeTruthy();
});
it("20 suggestions, a queue of 3 and 10 chips keep the textbox outside the capped containers", () => {
  const h = setup("permission");
  queue(h.store);
  mount(h.store);
  act(() => {
    for (let i = 0; i < 10; i++) h.store.set(addAttachmentAtom, source(i));
  });
  for (let i = 0; i < 20; i++) h.receive("review", { ...suggestion[0], id: `suggestion-${i}` });
  expect(box().parentElement?.closest(".overflow-y-auto")).toBeNull();
  expect(send().closest(".overflow-y-auto")).toBeNull();
  expect(screen.getByText("Queued (3)")).toBeTruthy();
});
it("read-only server mode line says Read-only and leaves the message to the banner", () => {
  const h = setup();
  h.store.set(authoringAtom, { holder: null });
  h.store.set(serverInfoAtom, (s) => ({
    ...s!,
    read_only: [{ what: "claude_login", message: "Claude is not logged in", fix: "Log in" }],
  }));
  mount(h.store);
  expect(screen.getByText(/Read-only/).textContent).toBe("Read-only");
  expect(screen.queryByText(/Claude is not logged in/)).toBeNull();
  expect(screen.queryByRole("textbox")).toBeNull();
});
it("server read-only mode also hides a holder's composer", () => {
  const h = setup();
  h.store.set(serverInfoAtom, (s) => ({
    ...s!,
    read_only: [{ what: "tmux", message: "tmux is unavailable", fix: "Install tmux" }],
  }));
  mount(h.store);
  expect(screen.getByText(/Read-only/).textContent).toBe("Read-only");
  expect(screen.queryByText(/tmux is unavailable/)).toBeNull();
  expect(screen.queryByRole("textbox")).toBeNull();
});
it("refill uses only queue_returned when the queue changed after Stop", async () => {
  const h = setup("working");
  queue(h.store);
  mount(h.store);
  type("Draft");
  fireEvent.click(screen.getByRole("button", { name: "Stop" }));
  act(() => queue(h.store, 1));
  expect(box().value).toBe("Draft");
  await h.result();
  expect(box().value).toBe("Draft");
  h.receive("chat", { frame: "queue_returned", texts: ["Authoritative", "New queue"] });
  await waitFor(() => expect(box().value).toBe("Draft\n\nAuthoritative\n\nNew queue"));
});
it("the texts ending a chat returns reach the draft although the concluded view has no composer", async () => {
  const h = setup("working");
  mount(h.store);
  void endChat(h.store);
  await h.result();
  act(() =>
    h.store.set(snapshotAtom, (s) => ({ ...s, session: { ...s.session!, state: "ended" } })),
  );
  expect(screen.queryByRole("textbox", { name: "Message" })).toBeNull();
  h.receive("chat", { frame: "queue_returned", texts: ["first", "second"] });
  expect(h.store.get(draftAtom).text).toBe("first\n\nsecond");
});
it("slash commands and the delivered attachment byte limit block sending", () => {
  const h = setup();
  mount(h.store);
  type(" /clear");
  expect(send().getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText("Slash commands are not available here; `/compact` is.")).toBeTruthy();
  type("/compact");
  expect(send().getAttribute("aria-disabled")).toBeNull();
  type("é".repeat(37_000) + "a".repeat(23_000));
  expect(send().getAttribute("aria-disabled")).toBeNull();
  act(() => h.store.set(addAttachmentAtom, { ...source(), text: "é".repeat(50_000) }));
  expect(send().getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText(/bytes over the 100,000-byte limit/)).toBeTruthy();
});
it("counts the sanitized delivered bytes and names the exact excess", () => {
  const h = setup();
  mount(h.store);
  type("a".repeat(100_000) + "\u0085".repeat(10));
  expect(send().getAttribute("aria-disabled")).toBeNull();
  type("a".repeat(100_001));
  expect(
    screen.getByText("Message with attachments is 1 byte over the 100,000-byte limit."),
  ).toBeTruthy();
});
it.each(["/compactly", "\u0001 /clear", "\u0085/clear", "\uFEFF/clear"])(
  "refuses disguised slash command %j",
  (text) => {
    const h = setup();
    mount(h.store);
    type(text);
    fireEvent.click(send());
    expect(h.sent).toEqual([]);
    expect(screen.getByText("Slash commands are not available here; `/compact` is.")).toBeTruthy();
  },
);
it("c opens a closed sheet, then focuses the composer of the shown panel", () => {
  const h = setup();
  h.store.set(chatSheetOpenAtom, false);
  mount(
    h.store,
    <>
      <ShortcutHost />
      <ComposerArea />
    </>,
  );
  expect(document.activeElement).not.toBe(box());
  fireEvent.keyDown(document.body, { key: "c" });
  expect(h.store.get(chatSheetOpenAtom)).toBe(true);
  expect(document.activeElement).toBe(box());
});
