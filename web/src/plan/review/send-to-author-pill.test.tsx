import { cleanup, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it } from "vitest";

import { AddToChat, MAX_ATTACHMENT_BYTES, registerSelectionSecondary } from "@/plan/attachments";
import type { AttachmentDraft } from "@/plan/attachments";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

import { registerSendToAuthor } from "./send-to-author";

const rect = { top: 0, left: 0, right: 10, bottom: 10, width: 10, height: 10 } as DOMRect;
const me = { public_id: "aaaaaaaaaaaaaaaa", name: "Me" };
const ada = { public_id: "bbbbbbbbbbbbbbbb", name: "Ada", away_since: null, frees_at: null };

const draft = (text = "words"): AttachmentDraft => ({
  kind: "source",
  path: "doc/notes.md",
  anchor: "purpose",
  text,
  lines: null,
  stageId: null,
  hash: "abc",
});

function mount(holder: typeof ada | null, given = draft()) {
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
  return render(
    <Provider store={store}>
      <AddToChat draft={given} rect={rect} onAdded={() => {}} />
    </Provider>,
  );
}

let unregister: () => void;
beforeEach(() => {
  unregister = registerSendToAuthor();
});
afterEach(() => {
  cleanup();
  unregister();
});

it("draws no rule after Add to chat when nobody holds authoring to send to", () => {
  const { container } = mount(null);
  expect(screen.getByRole("button", { name: "Add to chat" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Send to author" })).toBeNull();
  expect(container.querySelector(".add-to-chat__rule")).toBeNull();
});

it("draws no empty pill over the oversize note for a user who can author", () => {
  const { container } = mount(null, draft("a".repeat(MAX_ATTACHMENT_BYTES + 1)));
  expect(container.querySelector(".add-to-chat__pill")).toBeNull();
  expect(screen.getByRole("note").textContent).toMatch(/16 KiB/);
});

it("draws the pill with Send to author alone for a user another holder blocks", () => {
  const { container } = mount(ada);
  expect(screen.getByRole("button", { name: "Send to author" })).toBeTruthy();
  expect(container.querySelector(".add-to-chat__pill")).not.toBeNull();
  expect(container.querySelector(".add-to-chat__rule")).toBeNull();
});

it("skips the rule and the pill for any secondary that draws nothing", () => {
  const off = registerSelectionSecondary(() => null);
  try {
    const { container } = mount(null);
    expect(container.querySelector(".add-to-chat__rule")).toBeNull();
    cleanup();
    const oversize = mount(null, draft("a".repeat(MAX_ATTACHMENT_BYTES + 1)));
    expect(oversize.container.querySelector(".add-to-chat__pill")).toBeNull();
  } finally {
    off();
  }
});
