import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it, vi } from "vitest";

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
    mount(storeWith(ada));
    expect(screen.getByRole("button", { name: "Send words" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Add to chat" })).toBeNull();
    cleanup();
    unregister();
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
});
