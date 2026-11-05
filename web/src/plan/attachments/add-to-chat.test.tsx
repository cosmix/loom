import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import { sessionViewSchema } from "@/plan/api/schema-chat";
import { snapshotAtom } from "@/plan/chat/model";
import { screenAtom } from "@/plan/chat/session-home";
import { chatSheetOpenAtom } from "@/plan/chat/sheet-state";
import { authoringAtom, canAuthorAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import { AddToChat, MAX_ATTACHMENT_BYTES } from "./add-to-chat";
import { pendingAttachmentsAtom } from "./atoms";
import { registerSelectionSecondary } from "./secondary";
import type { AttachmentDraft } from "./types";

type Store = ReturnType<typeof createStore>;
const rect = { top: 0, left: 0, right: 10, bottom: 10, width: 10, height: 10 } as DOMRect;
const me = { public_id: "aaaaaaaaaaaaaaaa", name: "Me" };
const ada = { public_id: "bbbbbbbbbbbbbbbb", name: "Ada", away_since: null, frees_at: null };

function draft(text = "words"): AttachmentDraft {
  return {
    kind: "source",
    path: "doc/notes.md",
    anchor: "purpose",
    text,
    lines: null,
    stageId: null,
    hash: "abc",
  };
}

function storeWith(holder: typeof ada | null): Store {
  const store = createStore();
  store.set(serverInfoAtom, {
    project: "p",
    project_key: "p-1",
    port: 1,
    remote: false,
    read_only: [],
    stopping: false,
    you: me,
  });
  store.set(youAtom, me);
  store.set(authoringAtom, { holder });
  return store;
}

function mount(store: Store, given = draft(), onAdded = () => {}) {
  return render(
    <Provider store={store}>
      <AddToChat draft={given} rect={rect} onAdded={onAdded} />
    </Provider>,
  );
}

afterEach(cleanup);

describe("AddToChat", () => {
  it("shows the button for the holder and for a free plan, not for another holder", () => {
    const holder = storeWith({ ...ada, ...me });
    expect(holder.get(canAuthorAtom)).toBe(true);
    mount(holder);
    expect(screen.getByRole("button", { name: "Add to chat" })).toBeTruthy();
    cleanup();

    mount(storeWith(null));
    expect(screen.getByRole("button", { name: "Add to chat" })).toBeTruthy();
    cleanup();

    const blocked = storeWith(ada);
    expect(blocked.get(canAuthorAtom)).toBe(false);
    const { container } = mount(blocked);
    expect(container.textContent).toBe("");
  });

  it("opens the narrow sheet when a selection is added", () => {
    const store = storeWith(null);
    expect(store.get(chatSheetOpenAtom)).toBe(false);
    const onAdded = vi.fn();
    mount(store, draft(), onAdded);

    fireEvent.click(screen.getByRole("button", { name: "Add to chat" }));

    expect(store.get(pendingAttachmentsAtom)).toHaveLength(1);
    expect(store.get(chatSheetOpenAtom)).toBe(true);
    expect(onAdded).toHaveBeenCalledOnce();
  });

  it("hides the button while the open session belongs to another screen", () => {
    const store = storeWith(null);
    const session = sessionViewSchema.parse({
      ...snapshotFixture.session,
      state: "idle",
      feature: "alpha",
    });
    store.set(snapshotAtom, { session, turns: [], queue: [], previous: null });
    store.set(screenAtom, { feature: "beta", plan: null });
    const { container } = mount(store);
    expect(container.textContent).toBe("");
    cleanup();

    store.set(screenAtom, { feature: "alpha", plan: null });
    mount(store);
    expect(screen.getByRole("button", { name: "Add to chat" })).toBeTruthy();
  });

  it("keeps the button once the session that belongs to another screen has ended", () => {
    const store = storeWith(null);
    const session = sessionViewSchema.parse({
      ...snapshotFixture.session,
      state: "ended",
      feature: "alpha",
    });
    store.set(screenAtom, { feature: "beta", plan: null });
    store.set(snapshotAtom, {
      session: null,
      turns: [],
      queue: [],
      previous: { session, turns: [] },
    });
    mount(store);
    expect(screen.getByRole("button", { name: "Add to chat" })).toBeTruthy();
    cleanup();

    // The same session, still open, is the control: it hides the button.
    store.set(snapshotAtom, {
      session: { ...session, state: "idle" },
      turns: [],
      queue: [],
      previous: null,
    });
    const { container } = mount(store);
    expect(container.textContent).toBe("");
  });

  it("adds the draft and calls onAdded; the same draft twice keeps one", () => {
    const store = storeWith(null);
    const onAdded = vi.fn();
    mount(store, draft(), onAdded);
    const button = screen.getByRole("button", { name: "Add to chat" });
    fireEvent.click(button);
    fireEvent.click(button);
    expect(onAdded).toHaveBeenCalledTimes(2);
    expect(store.get(pendingAttachmentsAtom)).toEqual([draft()]);
  });

  it("renders a registered secondary without the button, and unregistering removes it", () => {
    const unregister = registerSelectionSecondary((d) => (
      <button type="button">Send {d.text}</button>
    ));
    try {
      mount(storeWith(ada));
      expect(screen.getByRole("button", { name: "Send words" })).toBeTruthy();
      expect(screen.queryByRole("button", { name: "Add to chat" })).toBeNull();
      cleanup();
    } finally {
      unregister();
    }
    const { container } = mount(storeWith(ada));
    expect(container.textContent).toBe("");
  });

  it("refuses text over 16 KiB with a note, and accepts exactly 16 KiB", () => {
    mount(storeWith(null), draft("a".repeat(MAX_ATTACHMENT_BYTES + 1)));
    expect(screen.queryByRole("button", { name: "Add to chat" })).toBeNull();
    expect(screen.getByRole("note").textContent).toMatch(/16 KiB/);
    cleanup();
    mount(storeWith(null), draft("a".repeat(MAX_ATTACHMENT_BYTES)));
    expect(screen.getByRole("button", { name: "Add to chat" })).toBeTruthy();
  });

  it("sets the oversize note under the pill, never inside it", () => {
    const oversize = draft("a".repeat(MAX_ATTACHMENT_BYTES + 1));
    // Alone: no pill is drawn, the note stands in the group by itself.
    const { container } = mount(storeWith(null), oversize);
    expect(container.querySelector(".add-to-chat__pill")).toBeNull();
    expect(screen.getByRole("note").closest(".add-to-chat")).not.toBeNull();
    cleanup();

    // Beside a secondary: the pill holds the secondary, the note sits under it.
    const unregister = registerSelectionSecondary(() => <button type="button">Send</button>);
    try {
      mount(storeWith(null), oversize);
      const pill = screen.getByRole("button", { name: "Send" }).closest(".add-to-chat__pill");
      expect(pill).not.toBeNull();
      expect(screen.getByRole("note").closest(".add-to-chat__pill")).toBeNull();
      expect(screen.queryByRole("button", { name: "Add to chat" })).toBeNull();
    } finally {
      unregister();
    }
  });
});
