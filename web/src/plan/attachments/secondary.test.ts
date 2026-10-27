import { describe, expect, it, vi } from "vitest";
import { registerSelectionSecondary, selectionSecondaries } from "./secondary";
import type { AttachmentDraft } from "./types";

const draft: AttachmentDraft = {
  kind: "source",
  path: "doc/notes.md",
  anchor: null,
  text: "Selected text",
  lines: null,
  stageId: null,
  hash: null,
};

describe("selection secondaries", () => {
  it("returns registered renderers in order and removes them independently", () => {
    const first = vi.fn(() => null);
    const second = vi.fn(() => null);
    const unregisterFirst = registerSelectionSecondary(first);
    const unregisterSecond = registerSelectionSecondary(second);
    try {
      expect(selectionSecondaries()).toEqual([first, second]);
      for (const render of selectionSecondaries()) render(draft);
      expect(first).toHaveBeenCalledWith(draft);
      expect(second).toHaveBeenCalledWith(draft);
      unregisterFirst();
      expect(selectionSecondaries()).toEqual([second]);
    } finally {
      unregisterFirst();
      unregisterSecond();
    }
    expect(selectionSecondaries()).toEqual([]);
  });

  it("keeps duplicate registrations independent and returns a snapshot", () => {
    const render = () => null;
    const unregisterFirst = registerSelectionSecondary(render);
    const unregisterSecond = registerSelectionSecondary(render);
    try {
      const snapshot = selectionSecondaries();
      snapshot.pop();
      expect(selectionSecondaries()).toEqual([render, render]);
      unregisterFirst();
      unregisterFirst();
      expect(selectionSecondaries()).toEqual([render]);
    } finally {
      unregisterFirst();
      unregisterSecond();
    }
    expect(selectionSecondaries()).toEqual([]);
  });
});
