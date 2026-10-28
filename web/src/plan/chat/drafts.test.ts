import { createStore, getDefaultStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import helloJson from "@/plan/api/fixtures/hello.json";
import { planMessageSchema } from "@/plan/api/schema";
import type { AttachmentDraft } from "@/plan/attachments/types";
import { serverInfoAtom } from "@/plan/state/atoms";
import { draftAtom } from "./drafts";
import { storageFullAtom, storageKey } from "./storage";

const hello = planMessageSchema.parse(helloJson);
if (hello.type !== "hello") throw new Error("hello.json is not a hello frame");
const server = hello.server;
const emptyDraft = { text: "", attachments: [] };
const attachments: AttachmentDraft[] = [
  {
    kind: "source",
    path: "doc/product/section_01.md",
    anchor: "scope",
    text: "Selected passage",
    lines: { start: 3, end: 5 },
    stageId: null,
    hash: "source-hash",
  },
  {
    kind: "summary",
    summary: "product",
    sentence: "The product overview",
    text: "Summary passage",
    restsOn: [{ path: "doc/product/section_01.md", anchor: "scope" }],
    hash: null,
  },
];
const savedDraft = { text: "Keep this draft", attachments };
const subscriptions: (() => void)[] = [];

function mountedStore(projectKey?: string) {
  const store = createStore();
  if (projectKey != null) store.set(serverInfoAtom, { ...server, project_key: projectKey });
  subscriptions.push(store.sub(draftAtom, () => {}));
  return store;
}

beforeEach(() => {
  localStorage.clear();
  getDefaultStore().set(storageFullAtom, false);
});

afterEach(() => {
  subscriptions.splice(0).forEach((unsubscribe) => unsubscribe());
  vi.restoreAllMocks();
  localStorage.clear();
  getDefaultStore().set(storageFullAtom, false);
});

it("keeps draft text and attachments in memory when storage writes fail", () => {
  const store = mountedStore("draft-write-failure");
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new DOMException("Storage is full", "QuotaExceededError");
  });

  expect(() => store.set(draftAtom, savedDraft)).not.toThrow();

  expect(store.get(draftAtom)).toEqual(savedDraft);
  expect(getDefaultStore().get(storageFullAtom)).toBe(true);
});

it.each([
  ["invalid JSON", "{"],
  ["null", "null"],
  ["wrong text type", JSON.stringify({ text: 42, attachments: [] })],
  ["missing attachments", JSON.stringify({ text: "Draft" })],
  [
    "invalid source",
    JSON.stringify({ text: "Draft", attachments: [{ ...attachments[0], lines: "3-5" }] }),
  ],
  [
    "invalid summary",
    JSON.stringify({ text: "Draft", attachments: [{ ...attachments[1], restsOn: ["doc/a.md"] }] }),
  ],
])("drops a stored draft with %s", (_label, stored) => {
  const projectKey = `invalid-${_label}`;
  localStorage.setItem(storageKey(projectKey, "draft"), stored);

  const store = mountedStore(projectKey);

  expect(store.get(draftAtom)).toEqual(emptyDraft);
});

it("preserves draft text and both attachment variants across a remount", () => {
  const store = mountedStore("draft-remount");
  store.set(draftAtom, savedDraft);
  subscriptions.pop()?.();

  const remounted = mountedStore("draft-remount");

  expect(remounted.get(draftAtom)).toEqual(savedDraft);
  expect(JSON.parse(localStorage.getItem(storageKey("draft-remount", "draft"))!)).toEqual(
    savedDraft,
  );
});

it("isolates drafts between projects", () => {
  const store = mountedStore("draft-project-a");
  store.set(draftAtom, savedDraft);

  store.set(serverInfoAtom, { ...server, project_key: "draft-project-b" });
  expect(store.get(draftAtom)).toEqual(emptyDraft);
  store.set(draftAtom, { text: "Project B", attachments: [] });
  store.set(serverInfoAtom, { ...server, project_key: "draft-project-a" });

  expect(store.get(draftAtom)).toEqual(savedDraft);
});

it("keeps a pre-hello draft in memory only", () => {
  const store = mountedStore();

  store.set(draftAtom, savedDraft);

  expect(store.get(draftAtom)).toEqual(savedDraft);
  expect(localStorage.length).toBe(0);
});

it("adopts a pre-hello draft under the project key without losing text", () => {
  const store = mountedStore();
  store.set(draftAtom, savedDraft);

  store.set(serverInfoAtom, { ...server, project_key: "draft-adopt" });

  expect(store.get(draftAtom)).toEqual(savedDraft);
  store.set(draftAtom, (draft) => ({ ...draft, text: `${draft.text} more` }));
  const keyed = storageKey("draft-adopt", "draft");
  expect(JSON.parse(localStorage.getItem(keyed)!)).toEqual({
    ...savedDraft,
    text: "Keep this draft more",
  });
  expect(localStorage.getItem("draft")).toBeNull();
  store.set(serverInfoAtom, { ...server, project_key: "draft-other" });
  expect(store.get(draftAtom)).toEqual(emptyDraft);
});

it("joins a pre-hello draft with the stored draft of the project", () => {
  localStorage.setItem(
    storageKey("draft-join", "draft"),
    JSON.stringify({ text: "Stored", attachments: [attachments[0]] }),
  );
  const store = mountedStore();
  store.set(draftAtom, { text: "Typed early", attachments: [attachments[1]] });

  store.set(serverInfoAtom, { ...server, project_key: "draft-join" });

  expect(store.get(draftAtom)).toEqual({
    text: "Stored\n\nTyped early",
    attachments,
  });
  store.set(draftAtom, emptyDraft);
  expect(store.get(draftAtom)).toEqual(emptyDraft);
});

it("drops invalid drafts delivered by storage events", () => {
  const projectKey = "draft-storage-event";
  const store = mountedStore(projectKey);
  store.set(draftAtom, savedDraft);

  window.dispatchEvent(
    new StorageEvent("storage", {
      key: storageKey(projectKey, "draft"),
      newValue: JSON.stringify({ text: "Broken", attachments: [{ kind: "unknown" }] }),
      storageArea: localStorage,
    }),
  );

  expect(store.get(draftAtom)).toEqual(emptyDraft);
});
