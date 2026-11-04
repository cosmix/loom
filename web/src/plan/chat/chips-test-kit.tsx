import { act, render } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { vi } from "vitest";
import { MemoryRouter, useLocation } from "react-router";

import chatJson from "@/plan/api/fixtures/chat-snapshot.json";
import { chatSnapshotSchema, type Attachment, type Item, type Turn } from "@/plan/api/schema-chat";
import { addAttachmentAtom, type AttachmentDraft, type ChatAttachment } from "@/plan/attachments";
import { serverInfoAtom } from "@/plan/state/atoms";
import { PendingChips, SentChips } from "./chips";
import { snapshotAtom } from "./model";

export type Store = ReturnType<typeof createStore>;
export const snapshot = chatSnapshotSchema.parse(chatJson);
export const baseTurn = snapshot.turns[0];
export const baseUser = baseTurn.items.find(
  (item): item is Extract<Item, { type: "user" }> => item.type === "user",
)!;
export const baseAttachment = baseUser.attachments[0];
let project = 0;

/** The project key `setup` gave the most recently created store. */
export function currentProjectKey(): string {
  return `chips-${project}`;
}

export function source(path = "doc/product/section_02.md"): AttachmentDraft {
  return {
    kind: "source",
    path,
    anchor: "Port",
    text: "The entire selected passage",
    lines: null,
    stageId: null,
    hash: "selected",
  };
}
export function summary(): Extract<AttachmentDraft, { kind: "summary" }> {
  return {
    kind: "summary",
    summary: "demo/product/section_01",
    sentence: "sentence-1",
    text: "The summary passage",
    restsOn: [],
    hash: null,
  };
}
export function setup() {
  const store = createStore();
  store.set(serverInfoAtom, {
    project: "p",
    project_key: `chips-${++project}`,
    port: 1,
    remote: false,
    stopping: false,
    read_only: [],
    you: { public_id: "me", name: "Me" },
  });
  store.set(snapshotAtom, snapshot);
  return store;
}
function Location() {
  const location = useLocation();
  return (
    <output aria-label="Location">
      {location.pathname}
      {location.search}
      {location.hash}
    </output>
  );
}
export function mount(store: Store, children = <PendingChips />, pathname = "/files") {
  return render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[pathname]}>
        {children}
        <Location />
      </MemoryRouter>
    </Provider>,
  );
}
export function add(store: Store, draft: ChatAttachment = source()) {
  act(() => store.set(addAttachmentAtom, draft));
}
export function sent(store: Store, attachments: Attachment[], turn: Turn = baseTurn) {
  return mount(store, <SentChips item={{ ...baseUser, attachments }} turn={turn} />);
}
export function sections(files: { path: string; anchors: string[] }[]) {
  return vi
    .spyOn(globalThis, "fetch")
    .mockResolvedValue(new Response(JSON.stringify({ turn: baseTurn.id, files })));
}
