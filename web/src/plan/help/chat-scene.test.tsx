import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { CHAT_SCENES, CHAT_TIPS, ChatBackgroundScene, ChatConversationScene } from "./chat-scene";

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

it("provides the composer, conversation and background scenes and concise tips", () => {
  expect(CHAT_SCENES.map(({ title }) => title)).toEqual([
    "The composer",
    "A conversation",
    "Background tasks",
  ]);
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

it("shows an idle chat waiting on two background tasks, with the agent row outside the fold", () => {
  const { container } = render(<ChatBackgroundScene />);

  expect(container.querySelector(".chat-background-words")?.textContent).toBe(
    "Waiting on 2 background tasks",
  );
  const list = container.querySelector(".chat-background-list");
  expect(list?.querySelectorAll("li")).toHaveLength(2);
  expect(list?.textContent).toContain("Survey the release goals");
  expect(list?.textContent).toContain("Run the link check");
  const row = container.querySelector('[aria-label="Chat transcript"] .chat-tool-head');
  expect(row?.textContent).toContain("running");
  expect(container.querySelector(".chat-tool-fold")).toBeNull();
  expect(container.querySelector(".chat-state-row")).toBeNull();
});

it("names the command menu's keys in the tips", () => {
  const menu = CHAT_TIPS.find((tip) => tip.includes("command list"));
  expect(menu).toBeDefined();
  for (const key of ["/", "Up and Down", "Enter or Tab", "Escape", "terminal-only", "disabled"]) {
    expect(menu).toContain(key);
  }
  expect(CHAT_TIPS.some((tip) => tip.includes("waits on background agents or shells"))).toBe(true);
});

it("keeps the sending, draft and stopping tip in one entry", () => {
  const sending = CHAT_TIPS.find((tip) => tip.includes("Shift+Enter"));
  expect(sending).toContain("edits made while it sends stay in the draft");
  expect(sending).toContain("stopping a turn returns its unsent queued messages");
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
