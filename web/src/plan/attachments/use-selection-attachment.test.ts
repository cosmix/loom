import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it } from "vitest";

import type { AttachmentDraft } from "./types";
import { useSelectionAttachment } from "./use-selection-attachment";

function draftFor(sel: Selection): AttachmentDraft {
  return {
    kind: "source",
    path: "doc/a.md",
    anchor: null,
    text: sel.toString(),
    lines: null,
    stageId: null,
    hash: "h1",
  };
}

function select(node: Node, start: number, end: number) {
  const range = document.createRange();
  range.setStart(node, start);
  range.setEnd(node, end);
  const selection = window.getSelection() as Selection;
  selection.removeAllRanges();
  selection.addRange(range);
  document.dispatchEvent(new Event("selectionchange"));
}

beforeAll(() => {
  // jsdom does not implement layout for ranges.
  Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
});

afterEach(() => {
  window.getSelection()?.removeAllRanges();
  cleanup();
  document.body.replaceChildren();
});

describe("useSelectionAttachment", () => {
  it("reports a draft for a selection inside the container and ignores one outside", () => {
    const inside = document.createElement("p");
    inside.textContent = "inside words";
    const outside = document.createElement("p");
    outside.textContent = "outside words";
    document.body.append(inside, outside);
    const { result } = renderHook(() => useSelectionAttachment({ current: inside }, draftFor));

    act(() => select(inside.firstChild as Node, 0, 6));
    expect(result.current.draft).toMatchObject({ text: "inside", hash: "h1" });
    expect(result.current.rect).not.toBeNull();

    act(() => select(outside.firstChild as Node, 0, 7));
    expect(result.current.draft).toBeNull();
  });

  it("clear drops the draft and the selection", () => {
    const el = document.createElement("p");
    el.textContent = "some words";
    document.body.append(el);
    const { result } = renderHook(() => useSelectionAttachment({ current: el }, draftFor));
    act(() => select(el.firstChild as Node, 0, 4));
    expect(result.current.draft).not.toBeNull();
    act(() => result.current.clear());
    expect(result.current.draft).toBeNull();
    expect(window.getSelection()?.isCollapsed).toBe(true);
  });
});
