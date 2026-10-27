import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { AttachmentDraft } from "./types";
import { useSelectionAttachment } from "./use-selection-attachment";

// What the selection's range reports; tests move it to simulate the text scrolling.
let rangeRect = new DOMRect(20, 100, 40, 12);

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

function mountSelected() {
  const column = document.createElement("div");
  const paragraph = document.createElement("p");
  paragraph.textContent = "some words here";
  column.append(paragraph);
  document.body.append(column);
  const provenance = vi.fn(draftFor);
  const hook = renderHook(() => useSelectionAttachment({ current: column }, provenance));
  const range = document.createRange();
  range.setStart(paragraph.firstChild as Node, 0);
  range.setEnd(paragraph.firstChild as Node, 4);
  const selection = window.getSelection() as Selection;
  selection.removeAllRanges();
  selection.addRange(range);
  act(() => {
    document.dispatchEvent(new Event("selectionchange"));
  });
  return { ...hook, column, paragraph, provenance };
}

beforeAll(() => {
  // jsdom does not implement layout for ranges.
  Range.prototype.getBoundingClientRect = () => rangeRect;
});

beforeEach(() => {
  rangeRect = new DOMRect(20, 100, 40, 12);
});

afterEach(() => {
  window.getSelection()?.removeAllRanges();
  cleanup();
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

describe("useSelectionAttachment while the page moves", () => {
  it("re-measures the rect when an inner scroll container scrolls", () => {
    const { result, paragraph } = mountSelected();
    expect(result.current.rect?.top).toBe(100);

    rangeRect = new DOMRect(20, 60, 40, 12);
    // Scroll does not bubble: only a capture listener on window sees this.
    act(() => {
      paragraph.dispatchEvent(new Event("scroll"));
    });
    expect(result.current.rect?.top).toBe(60);
    expect(result.current.draft).toMatchObject({ text: "some" });
  });

  it("re-measures the rect on window resize", () => {
    const { result } = mountSelected();
    rangeRect = new DOMRect(5, 100, 40, 12);
    act(() => {
      window.dispatchEvent(new Event("resize"));
    });
    expect(result.current.rect?.left).toBe(5);
  });

  it("reports a rect outside the viewport instead of dropping the draft", () => {
    const { result, paragraph } = mountSelected();
    rangeRect = new DOMRect(20, -300, 40, 12);
    act(() => {
      paragraph.dispatchEvent(new Event("scroll"));
    });
    expect(result.current.rect?.bottom).toBeLessThan(0);
    expect(result.current.draft).not.toBeNull();
  });

  it("keeps the same snapshot when the selection did not move, without re-running provenance", () => {
    const { result, paragraph, provenance } = mountSelected();
    const before = result.current.rect;
    const calls = provenance.mock.calls.length;
    act(() => {
      paragraph.dispatchEvent(new Event("scroll"));
    });
    expect(result.current.rect).toBe(before);
    expect(provenance.mock.calls.length).toBe(calls);
  });

  it("stops listening once the selection is cleared", () => {
    const remove = vi.spyOn(window, "removeEventListener");
    const { result } = mountSelected();
    act(() => result.current.clear());
    expect(result.current.rect).toBeNull();
    const removed = remove.mock.calls.map(([type]) => type);
    expect(removed).toContain("scroll");
    expect(removed).toContain("resize");
  });
});
