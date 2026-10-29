import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import {
  PRESSURE_SCENES,
  PRESSURE_TIPS,
  PressureHeaderScene,
  PressureRunScene,
} from "@/plan/help/pressure-scene";

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

it("exports the two pressure scenes in order", () => {
  expect(PRESSURE_SCENES.map(({ title, Scene }) => [title, Scene])).toEqual([
    ["The run header", PressureHeaderScene],
    ["Following a run", PressureRunScene],
  ]);
});

it.each(PRESSURE_SCENES)("renders $title with every note anchored", ({ Scene, notes }) => {
  const { container } = render(<Scene />);

  expect(container.querySelector("[data-loom-plan][inert]")).not.toBeNull();
  expect(container.querySelector(".chat-run-header")).not.toBeNull();
  expect(notes.length).toBeGreaterThanOrEqual(3);
  expect(notes.length).toBeLessThanOrEqual(6);
  for (const note of notes) {
    const selectors = typeof note.target === "string" ? [note.target] : note.target;
    expect(selectors.length).toBeGreaterThan(0);
    for (const selector of selectors) {
      // A note pins one element; a selector matching several would ring them all.
      expect(container.querySelectorAll(selector), selector).toHaveLength(1);
    }
  }
});

it.each(PRESSURE_SCENES)("gives $title nonempty single sentence notes", ({ notes }) => {
  for (const note of notes) {
    expect(note.text.trim()).toMatch(/^[^.!?]+[.!?]$/);
  }
});

it.each(PRESSURE_SCENES)("keeps $title free of network activity", async ({ Scene }) => {
  render(<Scene />);
  // Flush microtasks so mount effects cannot conceal a deferred request.
  await Promise.resolve();

  expect(globalThis.fetch).not.toHaveBeenCalled();
  expect(globalThis.WebSocket).not.toHaveBeenCalled();
});

it("provides one to three single sentence pressure facts", () => {
  expect(PRESSURE_TIPS.length).toBeGreaterThanOrEqual(1);
  expect(PRESSURE_TIPS.length).toBeLessThanOrEqual(3);
  for (const tip of PRESSURE_TIPS) {
    expect(tip.trim()).toMatch(/^[^.!?]+[.!?]$/);
  }
});
