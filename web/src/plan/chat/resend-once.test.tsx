import { act, fireEvent, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";

import type { Attachment, Item } from "@/plan/api/schema-chat";
import { authoringAtom, connectionAtom, serverInfoAtom, stoppingAtom } from "@/plan/state/atoms";
import { ComposerArea } from "./composer";
import {
  initial,
  installComposerLifecycle,
  mount,
  other,
  send,
  setup,
  type,
} from "./composer-harness";
import {
  applyChatFrame,
  CONNECTION_LOST,
  registerPending,
  rejectAllPending,
  RESEND_OFFLINE,
  snapshotAtom,
  unconfirmedSendsAtom,
} from "./model";
import { Transcript } from "./transcript";
import { toWire } from "./wire-input-test-kit";

installComposerLifecycle();
beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise<Response>(() => {})),
  );
});

type Harness = ReturnType<typeof setup>;
type UserItem = Extract<Item, { type: "user" }>;

const lost = (fields: Partial<UserItem> = {}): UserItem => ({
  type: "user",
  id: "lost",
  text: "Lost message",
  attachments: [],
  delivery: "undelivered",
  origin: null,
  ...fields,
});
const panel = (
  <>
    <Transcript />
    <ComposerArea />
  </>
);
const resends = () => screen.queryAllByRole("button", { name: "Resend" });
const sends = (h: Harness) => h.sent.filter((message) => message.command === "send");
const phase = (h: Harness, value: "open" | "reconnecting") =>
  act(() => h.store.set(connectionAtom, { phase: value, everOpen: true, lastMessageAt: null }));

/** The reconnect snapshot: one working chat holding `items`. */
function snapshot(h: Harness, items: Item[]) {
  h.receive("chat", {
    frame: "snapshot",
    session: { ...initial.session!, state: "working" },
    turns: [{ ...initial.turns[0], open: true, items }],
    queue: [],
    previous: null,
  });
}

it("a send the server accepted then marked undelivered offers one Resend and sends once", async () => {
  const h = setup("working");
  mount(h.store, panel);
  type("Lost message");
  fireEvent.click(send());
  phase(h, "reconnecting");
  await act(async () => {});
  phase(h, "open");
  snapshot(h, [lost()]);
  expect(h.store.get(unconfirmedSendsAtom)).toHaveLength(1);

  expect(resends()).toHaveLength(1);
  fireEvent.click(resends()[0]);
  expect(sends(h)).toHaveLength(2);
  await h.result();
  expect(h.store.get(unconfirmedSendsAtom)).toEqual([]);
  expect(resends()).toHaveLength(0);
  expect(sends(h)).toHaveLength(2);
});

it("an item Resend that is lost again leaves one Resend, not a second control", async () => {
  const h = setup("working");
  mount(h.store, panel);
  snapshot(h, [lost()]);
  expect(resends()).toHaveLength(1);
  fireEvent.click(resends()[0]);
  phase(h, "reconnecting");
  await act(async () => {});
  expect(h.store.get(unconfirmedSendsAtom)).toHaveLength(1);
  phase(h, "open");

  expect(resends()).toHaveLength(1);
  fireEvent.click(resends()[0]);
  expect(sends(h)).toHaveLength(2);
  await h.result();
  expect(h.store.get(unconfirmedSendsAtom)).toEqual([]);
  expect(resends()).toHaveLength(0);
});

/** An item Resend issued, then the connection drops before the result arrives. */
async function resendLostToDrop(h: Harness) {
  snapshot(h, [lost()]);
  fireEvent.click(resends()[0]);
  phase(h, "reconnecting");
  await act(async () => {});
  phase(h, "open");
  expect(h.store.get(unconfirmedSendsAtom)).toHaveLength(1);
}

it("a resend lost to a drop leaves one Resend when the snapshot holds the original and the copy", async () => {
  const h = setup("working");
  mount(h.store, panel);
  await resendLostToDrop(h);
  snapshot(h, [lost(), lost({ id: "copy" })]);

  expect(resends()).toHaveLength(1);
  fireEvent.click(resends()[0]);
  // The resend is in flight: one command, a busy transcript control, and no composer copy of it.
  expect(sends(h)).toHaveLength(2);
  expect(resends()).toHaveLength(1);
  const busy = resends()[0] as HTMLButtonElement;
  expect(busy.getAttribute("aria-disabled")).toBe("true");
  expect(document.getElementById(busy.getAttribute("aria-describedby") ?? "")?.textContent).toBe(
    "Resending",
  );
  expect(screen.queryByText(/Not delivered ·/)).toBeNull();
  fireEvent.click(busy);
  expect(sends(h)).toHaveLength(2);
  await h.result();
  expect(h.store.get(unconfirmedSendsAtom)).toEqual([]);
  expect(resends()).toHaveLength(0);
});

it("a resend lost to a drop offers no Resend once the snapshot shows the copy delivered", async () => {
  const h = setup("working");
  mount(h.store, panel);
  await resendLostToDrop(h);
  snapshot(h, [lost(), lost({ id: "copy", delivery: "delivered" })]);

  expect(h.store.get(unconfirmedSendsAtom)).toEqual([]);
  expect(resends()).toHaveLength(0);
  expect(screen.queryByText(/Not delivered ·/)).toBeNull();
});

it("a resend lost to a drop that the server never saw is offered once, from the composer", async () => {
  const h = setup("working");
  mount(h.store, panel);
  await resendLostToDrop(h);
  snapshot(h, [lost()]);

  expect(resends()).toHaveLength(1);
  expect(screen.getByText(/Not delivered ·/)).toBeTruthy();
});

it("a resend refused at once brings the control back and reports the refusal", async () => {
  const h = setup("working");
  mount(h.store, panel);
  snapshot(h, [lost()]);
  fireEvent.click(resends()[0]);
  await h.result("Not allowed");

  expect(resends()).toHaveLength(1);
  expect(screen.getByText("Not allowed").closest('[role="status"]')).not.toBeNull();
  fireEvent.click(resends()[0]);
  await h.result();
  expect(sends(h)).toHaveLength(2);
  expect(resends()).toHaveLength(0);
});

const passage = (path: string): Attachment => ({
  kind: "source",
  path,
  anchor: null,
  lines: null,
  stage: null,
  summary_sentence: null,
  rests_on: [],
  quote: `Quote of ${path}`,
  changed_since_selected: false,
  locked: null,
});
const wire = (chip: Attachment) => toWire(chip);

/** The entry for a send of "yes" with `attached` that a dropped connection left unconfirmed. */
function loseSend(h: Harness, attached: Attachment) {
  act(() => {
    registerPending(h.store, "lost-send", { text: "yes", attachments: [wire(attached)] }).catch(
      () => {},
    );
    rejectAllPending(h.store, CONNECTION_LOST, true);
  });
}

it("two undelivered messages with the same text and different attachments resend their own", async () => {
  const h = setup("working");
  mount(h.store, panel);
  const [x, y] = [passage("doc/x.md"), passage("doc/y.md")];
  loseSend(h, y);
  snapshot(h, [
    lost({ id: "first", text: "yes", attachments: [x] }),
    lost({ id: "second", text: "yes", attachments: [y] }),
  ]);
  expect(h.store.get(unconfirmedSendsAtom)).toHaveLength(1);
  expect(resends()).toHaveLength(2);

  fireEvent.click(resends()[0]);
  expect(sends(h)[0]).toMatchObject({ text: "yes", attachments: [wire(x)] });
  await h.result();
  expect(h.store.get(unconfirmedSendsAtom)).toHaveLength(1);

  expect(resends()).toHaveLength(1);
  fireEvent.click(resends()[0]);
  expect(sends(h)[1]).toMatchObject({ text: "yes", attachments: [wire(y)] });
  await h.result();
  expect(h.store.get(unconfirmedSendsAtom)).toEqual([]);
  expect(resends()).toHaveLength(0);
});

const file = { id: "up-1", name: "notes.md", media_type: "text/markdown", bytes: 12 };
const fileChip = (): Attachment => ({ ...passage(""), kind: "file", quote: "", file });

it("an undelivered message with a file chip resends the file by id", async () => {
  const h = setup("working");
  mount(h.store, panel);
  snapshot(h, [lost({ text: "see file", attachments: [fileChip()] })]);
  fireEvent.click(resends()[0]);
  // Exact shape: a spread chip adds path/quote/file, which the server refuses.
  expect(sends(h)[0]?.attachments).toEqual([{ kind: "file", id: "up-1", name: "notes.md" }]);
  await h.result();
});

it("a file send lost to a drop pairs with its undelivered item without crashing", async () => {
  const h = setup("working");
  mount(h.store, panel);
  const input = { kind: "file", id: "up-1", name: "notes.md" } as const;
  act(() => {
    registerPending(h.store, "lost-file", { text: "see file", attachments: [input] }).catch(
      () => {},
    );
    rejectAllPending(h.store, CONNECTION_LOST, true);
  });
  snapshot(h, [lost({ text: "see file", attachments: [fileChip()] })]);
  fireEvent.click(resends()[0]);
  await h.result();
  expect(h.store.get(unconfirmedSendsAtom)).toEqual([]);
});

it("a lost entry whose attachments match no undelivered item keeps the composer Resend", async () => {
  const h = setup("working");
  mount(h.store, panel);
  const x = passage("doc/x.md");
  loseSend(h, passage("doc/y.md"));
  snapshot(h, [lost({ text: "yes", attachments: [x] })]);
  expect(resends()).toHaveLength(2);

  // Resending x makes the server show 'yes' + x; that is not delivery of the lost 'yes' + y.
  fireEvent.click(resends()[0]);
  act(() =>
    applyChatFrame(h.store, {
      frame: "delta",
      op: "item_added",
      turn_id: h.store.get(snapshotAtom).turns[0]!.id,
      item: lost({ id: "sent-x", text: "yes", attachments: [x], delivery: "sent" }),
    }),
  );
  await h.result();
  expect(h.store.get(unconfirmedSendsAtom)).toHaveLength(1);
  expect(resends()).toHaveLength(1);
  expect(screen.getByText(/Not delivered ·/)).toBeTruthy();
});

it("the offline Resend stays focusable and names why it cannot be used", () => {
  const h = setup("working");
  mount(h.store, panel);
  snapshot(h, [lost()]);
  phase(h, "reconnecting");

  const button = resends()[0] as HTMLButtonElement;
  button.focus();
  expect(document.activeElement).toBe(button);
  expect(button.disabled).toBe(false);
  expect(button.getAttribute("aria-disabled")).toBe("true");
  const reason = document.getElementById(button.getAttribute("aria-describedby") ?? "");
  expect(reason?.textContent).toBe(RESEND_OFFLINE);
  fireEvent.click(button);
  expect(sends(h)).toHaveLength(0);

  phase(h, "open");
  expect(resends()[0].getAttribute("aria-disabled")).toBeNull();
});

it("a lost entry for a message with no undelivered item keeps the composer Resend", async () => {
  const h = setup("working");
  mount(h.store, panel);
  type("Lost message");
  fireEvent.click(send());
  phase(h, "reconnecting");
  await act(async () => {});
  phase(h, "open");
  snapshot(h, []);
  expect(resends()).toHaveLength(1);
  expect(screen.getByText(/Not delivered ·/)).toBeTruthy();
});

it("a resent message stays resent after the panel remounts", async () => {
  const h = setup("working");
  const view = mount(h.store, panel);
  snapshot(h, [lost()]);
  fireEvent.click(resends()[0]);
  await h.result();
  expect(resends()).toHaveLength(0);

  view.unmount();
  mount(h.store, panel);
  expect(screen.getByText("Not delivered")).toBeTruthy();
  expect(resends()).toHaveLength(0);
});

const gates: Array<[string, (h: Harness) => void]> = [
  [
    "the chat is closing",
    (h) =>
      h.store.set(snapshotAtom, (s) => ({ ...s, session: { ...s.session!, state: "ending" } })),
  ],
  [
    "the chat has ended",
    (h) => h.store.set(snapshotAtom, (s) => ({ ...s, session: { ...s.session!, state: "ended" } })),
  ],
  ["loom plan is stopping", (h) => h.store.set(stoppingAtom, true)],
  [
    "the server is read-only",
    (h) =>
      h.store.set(serverInfoAtom, (s) => ({
        ...s!,
        read_only: [{ what: "tmux", message: "tmux is unavailable", fix: "Install tmux" }],
      })),
  ],
  ["another browser holds authoring", (h) => h.store.set(authoringAtom, { holder: other })],
];

it.each(gates)("no Resend on an undelivered message when %s", (_name, apply) => {
  const h = setup("working");
  mount(h.store, panel);
  snapshot(h, [lost()]);
  expect(resends()).toHaveLength(1);
  act(() => apply(h));
  expect(screen.getByText("Not delivered")).toBeTruthy();
  expect(resends()).toHaveLength(0);
});

it("an undelivered message offers Resend while nobody holds authoring, as the composer does", () => {
  const h = setup("working");
  mount(h.store, panel);
  snapshot(h, [lost()]);
  act(() => h.store.set(authoringAtom, { holder: null }));
  expect(resends()).toHaveLength(1);
});

it("no Resend on an undelivered message of the previous chat", () => {
  const h = setup("working");
  mount(h.store, panel);
  h.receive("chat", {
    frame: "snapshot",
    session: null,
    turns: [],
    queue: [],
    previous: {
      session: initial.session!,
      turns: [{ ...initial.turns[0], open: false, items: [lost()] }],
    },
  });
  expect(screen.getByText("Not delivered")).toBeTruthy();
  expect(resends()).toHaveLength(0);
});

it.each(["offer", "start", "closing"] as const)(
  "no Resend on the server's %s message",
  (origin) => {
    const h = setup("working");
    mount(h.store, panel);
    snapshot(h, [lost({ origin })]);
    expect(screen.getByText("Not delivered")).toBeTruthy();
    expect(resends()).toHaveLength(0);
  },
);
