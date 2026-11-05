import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { CHAT_SCENES, CHAT_TIPS, ChatConversationScene } from "./chat-scene";

beforeEach(() => {
  vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("Help must not request live data")));
  vi.stubGlobal("WebSocket", vi.fn());
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: query === "(min-width: 1024px)",
    media: query,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  }));
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("provides the composer and conversation scenes and concise tips", () => {
  expect(CHAT_SCENES.map(({ title }) => title)).toEqual(["The composer", "A conversation"]);
  expect(CHAT_TIPS.length).toBeGreaterThanOrEqual(1);
  expect(CHAT_TIPS.length).toBeLessThanOrEqual(3);
  for (const tip of CHAT_TIPS) expect(tip).toMatch(/^[^.!?]+[.!?]$/u);
});

// Each note rings one control, so a selector that also matched a neighbour would widen the ring.
it.each(CHAT_SCENES)("rings exactly one element per note in $title", ({ Scene, notes }) => {
  const { container } = render(<Scene />);
  for (const { target } of notes) {
    const selectors = typeof target === "string" ? [target] : target;
    const matched = new Set(
      selectors.flatMap((selector) => [...container.querySelectorAll(selector)]),
    );
    expect(matched.size, selectors.join(", ")).toBe(1);
  }
});

it("draws a single warning glyph, inside the state line", () => {
  const { container } = render(<ChatConversationScene />);
  const stateLine = container.querySelector(".chat-state-line");

  expect(stateLine?.textContent).toContain("Needs your answer");
  expect(container.textContent?.split("⚠")).toHaveLength(2);
  expect(stateLine?.textContent?.split("⚠")).toHaveLength(2);
});

for (const { title, Scene, notes } of CHAT_SCENES) {
  it(`renders ${title} with every pin anchored to production elements`, () => {
    const { container } = render(<Scene />);

    expect(container.querySelector("[data-loom-plan][inert]")).not.toBeNull();
    expect(notes.length).toBeGreaterThanOrEqual(3);
    expect(notes.length).toBeLessThanOrEqual(6);
    for (const note of notes) {
      expect(note.text.trim()).toMatch(/^[^.!?]+[.!?]$/u);
      const selectors = typeof note.target === "string" ? [note.target] : note.target;
      expect(selectors.length).toBeGreaterThan(0);
      for (const selector of selectors) {
        expect(container.querySelector(selector), selector).not.toBeNull();
      }
    }
    expect(globalThis.fetch).not.toHaveBeenCalled();
    expect(globalThis.WebSocket).not.toHaveBeenCalled();
  });
}
