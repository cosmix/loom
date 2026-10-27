import { createStore } from "jotai/vanilla";
import { describe, expect, it } from "vitest";

import { addAttachmentAtom, pendingAttachmentsAtom } from "./atoms";
import type { AttachmentDraft } from "./types";

const source: AttachmentDraft = {
  kind: "source",
  path: "doc/product/section_06.md",
  anchor: "heading",
  text: "selected text",
  lines: { start: 2, end: 3 },
  stageId: null,
  hash: "sha256:one",
};

const summary: AttachmentDraft = {
  kind: "summary",
  summary: "product/section_05",
  sentence: "s-3fa9c2d1",
  text: "A sentence.",
  restsOn: [{ path: "doc/product/section_05.md", anchor: "heading" }],
  hash: null,
};

describe("attachment atoms", () => {
  it("deduplicates source drafts by value and retains distinct metadata", () => {
    const store = createStore();
    expect(store.get(pendingAttachmentsAtom)).toEqual([]);
    store.set(addAttachmentAtom, source);
    store.set(addAttachmentAtom, { ...source, lines: { end: 3, start: 2 } });
    expect(store.get(pendingAttachmentsAtom)).toEqual([source]);

    const changed = { ...source, hash: "sha256:two" };
    store.set(addAttachmentAtom, changed);
    store.set(addAttachmentAtom, { ...source, lines: null });
    expect(store.get(pendingAttachmentsAtom)).toEqual([
      source,
      changed,
      { ...source, lines: null },
    ]);
  });

  it("deduplicates summary references by value and preserves different references", () => {
    const store = createStore();
    store.set(addAttachmentAtom, summary);
    store.set(addAttachmentAtom, {
      ...summary,
      restsOn: [{ anchor: "heading", path: "doc/product/section_05.md" }],
    });
    const changed = {
      ...summary,
      restsOn: [{ path: "doc/product/section_05.md", anchor: "other" }],
    };
    store.set(addAttachmentAtom, changed);
    store.set(addAttachmentAtom, source);
    expect(store.get(pendingAttachmentsAtom)).toEqual([summary, changed, source]);
  });
});
